"""Private worker for OpenAI's public-client ChatGPT plan authorization."""
import base64
import fcntl
import hashlib
import hmac
import http.server
import json
import os
from pathlib import Path
import re
import secrets
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

ISSUER = "https://auth.openai.com"
API = "https://api.openai.com/v1"
TOKEN = ISSUER + "/api/accounts/oauth/token"
DISCOVERY = ISSUER + "/.well-known/openid-configuration"
SCOPE = "chatgpt.tokens.use.direct"
SCOPES = "openid profile email offline_access resource.invoke " + SCOPE
DYNAMIC = "dynamic_agent_client"
LIMIT = 1024 * 1024


class Problem(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


def request(url, *, form=None, body=None, token=None, timeout=15):
    headers = {"Accept": "application/json"}
    data = None
    if form is not None:
        data = urllib.parse.urlencode(form).encode()
        headers["Content-Type"] = "application/x-www-form-urlencoded"
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(url, data, headers)
    return urllib.request.build_opener(NoRedirect()).open(request, timeout=timeout)


def document(url, **kwargs):
    with request(url, **kwargs) as response:
        raw = response.read(LIMIT + 1)
    if len(raw) > LIMIT:
        raise Problem("ChatGPT response exceeded the size limit")
    return json.loads(raw) if raw else {}


def b64(raw):
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def unb64(value):
    return base64.urlsafe_b64decode(value + "=" * (-len(value) % 4))


def identity(token, client, nonce, access):
    try:
        from cryptography.hazmat.primitives import hashes
        from cryptography.hazmat.primitives.asymmetric import padding, rsa
    except ImportError as exc:
        raise Problem("ChatGPT sign-in needs Python cryptography") from exc
    try:
        header, payload, signature = token.split(".")
        claims = json.loads(unb64(payload))
        header_data = json.loads(unb64(header))
        kid = header_data["kid"]
        if header_data["alg"] != "RS256":
            raise ValueError("algorithm")
        keys = document(ISSUER + "/.well-known/jwks.json")["keys"]
        jwk, = [k for k in keys if k.get("kid") == kid and k.get("kty") == "RSA"
                and k.get("use", "sig") == "sig" and k.get("alg", "RS256") == "RS256"]
        number = lambda field: int.from_bytes(unb64(jwk[field]), "big")
        key = rsa.RSAPublicNumbers(number("e"), number("n")).public_key()
        key.verify(unb64(signature), (header + "." + payload).encode(),
                   padding.PKCS1v15(), hashes.SHA256())
        audience = claims["aud"]
        if isinstance(audience, str):
            audience = [audience]
        now = time.time()
        if (claims["iss"] != ISSUER or not isinstance(audience, list) or client not in audience
                or (len(audience) > 1 and claims.get("azp") != client)
                or claims.get("azp", client) != client or claims["nonce"] != nonce
                or type(claims["exp"]) is not int or claims["exp"] <= now
                or type(claims["iat"]) is not int or claims["iat"] > now + 5
                or not isinstance(claims["sub"], str) or not claims["sub"]):
            raise ValueError("claims")
        if "at_hash" in claims:
            if not hmac.compare_digest(claims["at_hash"], b64(hashlib.sha256(access.encode()).digest()[:16])):
                raise ValueError("access token hash")
        return claims
    except Exception as exc:
        raise Problem("Could not validate the ChatGPT sign-in identity") from exc


def load(path):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return {"host": "urn:uuid:" + str(uuid.uuid4()), "active": "", "accounts": []}
    with os.fdopen(fd) as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise Problem("ChatGPT account file must be a private regular file (mode 0600)")
        return json.loads(source.read(LIMIT + 1))


def save(path, state):
    fd, temporary = tempfile.mkstemp(prefix=".lofiji-chatgpt-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as target:
            json.dump(state, target)
            target.flush()
            os.fsync(target.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


class Callback(http.server.BaseHTTPRequestHandler):
    def setup(self):
        self.request.settimeout(1)
        super().setup()

    def log_message(self, *args):
        pass

    def do_GET(self):
        target = urllib.parse.urlsplit(self.path)
        if target.path != "/auth/callback" or self.headers.get("Host") != self.server.authority:
            self.send_error(404)
            return
        fields = urllib.parse.parse_qs(target.query, max_num_fields=30)
        if fields.get("state") != [self.server.state] or any(len(values) != 1 for values in fields.values()):
            self.send_error(400, "Invalid sign-in state")
            return
        self.server.result = {name: values[0] for name, values in fields.items()}
        body = b"You can close this tab and return to Lofiji to see your sign-in status."
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Referrer-Policy", "no-referrer")
        self.end_headers()
        self.wfile.write(body)


def authorize(host, existing):
    verifier = secrets.token_urlsafe(48)
    nonce = secrets.token_urlsafe(32)
    client = existing["client"] if existing else DYNAMIC
    with http.server.HTTPServer(("127.0.0.1", 0), Callback) as server:
        server.authority = "127.0.0.1:" + str(server.server_port)
        server.state = secrets.token_urlsafe(32)
        server.result = None
        server.timeout = 0.25
        redirect = "http://" + server.authority + "/auth/callback"
        params = {
            "client_id": client, "response_type": "code", "redirect_uri": redirect,
            "ext_agent_host_id": host, "scope": SCOPES, "resource": API,
            "state": server.state, "nonce": nonce, "code_challenge_method": "S256",
            "code_challenge": b64(hashlib.sha256(verifier.encode()).digest()),
        }
        if not existing:
            params["agent_name_hint"] = "Lofiji"
        elif existing.get("email"):
            params["login_hint"] = existing["email"]
        try:
            browser = subprocess.Popen(["xdg-open", ISSUER + "/api/accounts/authorize?"
                                        + urllib.parse.urlencode(params)],
                                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                       stderr=subprocess.DEVNULL, start_new_session=True)
        except FileNotFoundError as exc:
            raise Problem("Install xdg-open to start ChatGPT sign-in in your browser") from exc
        deadline = time.monotonic() + 240
        while server.result is None and time.monotonic() < deadline:
            if browser.poll() not in (None, 0):
                raise Problem("Could not open your browser for ChatGPT sign-in")
            server.handle_request()
        result = server.result
    if result is None:
        raise Problem("ChatGPT sign-in timed out")
    if result.get("error") or not result.get("code"):
        raise Problem("ChatGPT plan access was declined or unavailable")
    issued = result.get("client_id", client)
    if not re.fullmatch(r"oaiapp_[A-Za-z0-9_-]{1,200}", issued) or (existing and issued != client):
        raise Problem("ChatGPT did not return the expected client registration")
    tokens = document(TOKEN, form={
        "grant_type": "authorization_code", "client_id": issued, "code": result["code"],
        "code_verifier": verifier, "redirect_uri": redirect, "resource": API,
    })
    claims = identity(tokens.get("id_token", ""), issued, nonce, tokens.get("access_token", ""))
    if existing and claims["sub"] != existing["subject"]:
        raise Problem("The browser signed in to a different ChatGPT account")
    return {
        "client": issued, "subject": claims["sub"], "email": claims.get("email", ""),
        "id_token": tokens["id_token"], **token_fields(tokens),
    }


def token_fields(response, previous=None):
    scopes = response.get("scope", previous.get("scope", "") if previous else "")
    if SCOPE not in scopes.split():
        raise Problem("ChatGPT plan use was not granted in the browser")
    access, refresh = response.get("access_token"), response.get("refresh_token")
    if refresh is None and previous:
        refresh = previous["refresh"]
    expires = response.get("expires_in")
    if (not isinstance(access, str) or not access or not isinstance(refresh, str) or not refresh
            or type(expires) is not int or expires <= 0
            or response.get("token_type", "").lower() != "bearer"):
        raise Problem("ChatGPT returned incomplete session credentials")
    return {"access": access, "refresh": refresh, "expiry": time.time() + expires, "scope": scopes}


def access_token(path, state, account):
    if not account or not account.get("refresh"):
        raise Problem("Open Settings and select Continue with ChatGPT first")
    if account["expiry"] <= time.time() + 60:
        try:
            reply = document(TOKEN, form={
                "grant_type": "refresh_token", "client_id": account["client"],
                "refresh_token": account["refresh"], "resource": API,
            })
        except urllib.error.HTTPError as exc:
            if exc.code in (400, 401, 403):
                raise Problem("ChatGPT session expired. Reconnect from Settings.") from exc
            raise
        account.update(token_fields(reply, account))
        save(path, state)
    return account["access"]


def status(account):
    if not account:
        return "No ChatGPT account"
    email = "".join(c for c in str(account["email"]) if c.isprintable())[:100]
    label = (email or "ChatGPT") + " · " + account["client"][-8:]
    return "Connected · " + label if account.get("refresh") else label + " · signed out"


def list_models(token, timeout=15):
    catalog = document(API + "/models", token=token, timeout=timeout)
    models = {}
    for entry in catalog.get("models", []):
        slug = entry.get("slug")
        if (entry.get("visibility") != "list" or not isinstance(slug, str)
                or not re.fullmatch(r"[a-zA-Z0-9._:/-]{1,200}", slug)):
            continue
        name = entry.get("display_name")
        if isinstance(name, str):
            name = "".join(char for char in name if char.isprintable()).strip()[:100]
        models.setdefault(slug, {"slug": slug, "name": name or slug})
    if not models:
        raise Problem("No models are available to this ChatGPT account")
    return list(models.values())


def generate(token, instructions, messages, model, timeout, effort=None):
    if not model:
        model = list_models(token, timeout)[0]["slug"]
    body = {"model": model, "instructions": instructions, "input": messages,
            "store": False, "stream": True}
    if effort is not None:
        body["reasoning"] = {"effort": effort}

    output = []
    remaining = LIMIT
    with request(API + "/responses", body=body, token=token, timeout=timeout) as response:
        while raw := response.readline(remaining + 1):
            remaining -= len(raw)
            if remaining < 0:
                raise Problem("ChatGPT response exceeded the size limit")
            if not raw.startswith(b"data:"):
                continue
            data = raw[5:].strip()
            if data == b"[DONE]":
                break
            event = json.loads(data)
            kind = event.get("type")
            if kind in ("response.output_text.delta", "response.refusal.delta"):
                output.append(event.get("delta", ""))
            elif kind == "response.completed":
                text = "".join(output).strip()
                if not text:
                    raise Problem("ChatGPT returned no visible text")
                return text[:12000]
            elif kind in ("response.failed", "response.incomplete", "error"):
                code = (event.get("response", {}).get("error") or {}).get("code", event.get("code"))
                if code in ("subscription_sharing_usage_limit_exceeded", "rate_limit_exceeded"):
                    raise Problem("ChatGPT plan usage limit reached. Check ChatGPT Settings → Usage.")
                if code == "subscription_sharing_usage_unavailable":
                    raise Problem("ChatGPT plan access is unavailable. Check ChatGPT Settings → Usage.")
                raise Problem("ChatGPT could not complete this response")
    raise Problem("ChatGPT response ended before completion")


def revoke(account):
    discovery = document(DISCOVERY, timeout=5)
    endpoint = discovery.get("revocation_endpoint")
    if not endpoint or not endpoint.startswith(ISSUER + "/"):
        return False
    form = {"token": account["refresh"], "token_type_hint": "refresh_token",
            "client_id": account["client"]}
    for delay in (0, 0.5, 1):
        time.sleep(delay)
        try:
            with request(endpoint, form=form, timeout=5) as response:
                return response.status == 200
        except urllib.error.HTTPError as exc:
            if exc.code < 500:
                return False
        except (OSError, urllib.error.URLError):
            continue
    return False


def operation(path, command):
    state = load(path)
    accounts = state["accounts"]
    active = next((a for a in accounts if a["client"] == state["active"]), None)
    action = command["action"]
    if action == "status":
        return status(active)
    if action in ("connect", "add"):
        try:
            import cryptography  # Check before opening the browser.
        except ImportError as exc:
            raise Problem("ChatGPT sign-in needs Python cryptography") from exc
        save(path, state)  # Persist the host identifier before registration.
        account = authorize(state["host"], active if action == "connect" else None)
        accounts[:] = [a for a in accounts if a["client"] != account["client"]]
        accounts.append(account)
        state["active"] = account["client"]
        save(path, state)
        return status(account)
    if action == "switch":
        if not accounts:
            return status(None)
        index = next((i for i, a in enumerate(accounts) if a is active), -1)
        account = accounts[(index + 1) % len(accounts)]
        state["active"] = account["client"]
        save(path, state)
        return status(account)
    if action == "disconnect":
        if not active or not active.get("refresh"):
            return status(active)
        try:
            confirmed = revoke(active)
        except (OSError, ValueError):
            confirmed = False
        for field in ("access", "refresh", "expiry", "scope", "id_token"):
            active.pop(field, None)
        save(path, state)
        label = status(active)
        return label if confirmed else label + " · check ChatGPT Settings → Usage"
    if action == "models":
        return json.dumps(list_models(access_token(path, state, active)))
    if action == "chat":
        token = access_token(path, state, active)
        effort = command.get("thinking_effort")
        try:
            return generate(token, command["instructions"], command["messages"], command["model"],
                            command["timeout"], effort)
        except urllib.error.HTTPError as exc:
            if effort and exc.code in (400, 422):
                raise Problem("Model rejected the request. Check model support or try Model default for thinking effort.") from exc
            raise
    raise Problem("Unknown ChatGPT operation")


def run(command):
    path = Path(command["path"])
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(str(path) + ".lock", os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w"):
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise Problem("ChatGPT is busy in another Lofiji window") from exc
        return operation(path, command)


def main():
    try:
        result = {"text": run(json.load(sys.stdin))}
    except Problem as exc:
        result = {"error": str(exc)}
    except urllib.error.HTTPError as exc:
        messages = {401: "Reconnect ChatGPT in Settings", 403: "ChatGPT plan access was denied",
                    404: "This model is unavailable to your ChatGPT account",
                    429: "ChatGPT usage limit reached. Check ChatGPT Settings → Usage."}
        result = {"error": messages.get(exc.code, f"ChatGPT request failed (HTTP {exc.code})")}
    except (TimeoutError, urllib.error.URLError, OSError):
        result = {"error": "Could not complete the ChatGPT operation. Check your connection and try again."}
    except Exception:
        result = {"error": "Could not read ChatGPT account data or response"}
    print(json.dumps(result))


if __name__ == "__main__":
    main()
