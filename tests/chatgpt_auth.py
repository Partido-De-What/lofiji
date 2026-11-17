"""Exercise the ChatGPT plan flow with a real loopback callback and fake OpenAI replies."""
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlencode, urlsplit
from urllib.request import urlopen
from urllib.error import HTTPError

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding, rsa

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
import chatgpt


class Flow(unittest.TestCase):
    def setUp(self):
        self.key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        self.params = {}
        self.fresh = 0
        self.client = "oaiapp_lofiji_test"
        self.token = "fake_access_token"
        self.saw_revocation = False

    def token_reply(self, nonce):
        header = chatgpt.b64(b'{"alg":"RS256","kid":"test"}')
        claims = {
            "iss": chatgpt.ISSUER, "aud": [self.client], "sub": "example-subject",
            "email": "test@example.invalid", "iat": int(time.time()),
            "exp": int(time.time()) + 300, "nonce": nonce,
        }
        payload = chatgpt.b64(json.dumps(claims).encode())
        signed = (header + "." + payload).encode()
        signature = self.key.sign(signed, padding.PKCS1v15(), hashes.SHA256())
        return {
            "id_token": header + "." + payload + "." + chatgpt.b64(signature),
            "access_token": self.token, "refresh_token": "initial-refresh",
            "scope": chatgpt.SCOPES, "expires_in": 300, "token_type": "Bearer",
        }

    def browser(self, argv, **kwargs):
        self.params = {key: value[0] for key, value in parse_qs(urlsplit(argv[1]).query).items()}
        self.assertEqual(self.params["code_challenge_method"], "S256")

        def callback():
            redirect = self.params["redirect_uri"]
            try:
                urlopen(redirect + "?state=wrong&code=invalid", timeout=2).close()
            except HTTPError as error:
                self.assertEqual(error.code, 400)
                error.close()
            payload = {"state": self.params["state"], "code": "sample-code"}
            if self.params["client_id"] == chatgpt.DYNAMIC:
                payload["client_id"] = self.client
            with urlopen(redirect + "?" + urlencode(payload), timeout=2) as response:
                self.assertEqual(response.status, 200)

        self.callback = threading.Thread(target=callback)
        self.callback.start()

        class Process:
            def poll(self):
                return 0

        return Process()

    def document(self, url, **kwargs):
        if url.endswith("jwks.json"):
            public = self.key.public_key().public_numbers()
            return {"keys": [{
                "kid": "test", "kty": "RSA", "n": chatgpt.b64(public.n.to_bytes(256, "big")),
                "e": chatgpt.b64(public.e.to_bytes(3, "big")),
            }]}
        if url == chatgpt.TOKEN:
            form = kwargs["form"]
            self.assertEqual(form["resource"], chatgpt.API)
            self.assertEqual(form["client_id"], self.client)
            if form["grant_type"] == "refresh_token":
                self.assertEqual(form["refresh_token"], "initial-refresh")
                self.fresh += 1
                return {
                    "access_token": "rotated-access", "refresh_token": "rotated-refresh",
                    "expires_in": 300, "token_type": "Bearer",
                }
            self.assertEqual(form["redirect_uri"], self.params["redirect_uri"])
            self.assertEqual(form["code"], "sample-code")
            self.assertEqual(chatgpt.b64(hashlib.sha256(form["code_verifier"].encode()).digest()),
                             self.params["code_challenge"])
            return self.token_reply(self.params["nonce"])
        if url == chatgpt.API + "/models":
            self.assertEqual(kwargs["token"], "rotated-access")
            return {"models": [{"slug": "hidden", "visibility": "hidden"},
                               {"slug": "example", "visibility": "list", "display_name": "Example Model"},
                               {"slug": "gpt-6-luna", "visibility": "list", "display_name": "Second Option"}]}
        if url == chatgpt.DISCOVERY:
            return {"revocation_endpoint": chatgpt.ISSUER + "/revoke"}
        self.fail(url)

    def request(self, url, **kwargs):
        if url.endswith("/revoke"):
            self.assertEqual(kwargs["form"]["token"], "rotated-refresh")
            self.assertEqual(kwargs["form"]["client_id"], self.client)
            self.saw_revocation = True
            response = io.BytesIO()
            response.status = 200
            return response
        self.assertEqual(url, chatgpt.API + "/responses")
        self.assertEqual(kwargs["token"], "rotated-access")
        expected = {
            "model": "example", "instructions": "Planning context",
            "input": [{"role": "user", "content": "What next?"}],
            "store": False, "stream": True,
        }
        if getattr(self, "expect_effort", False):
            expected["reasoning"] = {"effort": "high"}
        self.assertEqual(kwargs["body"], expected)
        return io.BytesIO(b'data: {"type":"response.output_text.delta","delta":"Review"}\n\n'
                          b'data: {"type":"response.output_text.delta","delta":" your notes."}\n\n'
                          b'data: {"type":"response.completed"}\n\n')

    def test_sign_in_refresh_chat_and_sign_out(self):
        with tempfile.TemporaryDirectory() as folder, \
             patch.object(chatgpt.subprocess, "Popen", self.browser), \
             patch.object(chatgpt, "document", self.document), \
             patch.object(chatgpt, "request", self.request):
            path = Path(folder) / "settings.chatgpt.json"
            def action(name, **kwargs):
                return chatgpt.run({"path": os.fspath(path), "action": name, **kwargs})

            self.assertIn("Connected", action("connect"))
            self.callback.join(timeout=3)
            self.assertEqual(self.params["client_id"], chatgpt.DYNAMIC)
            self.assertEqual(self.params["agent_name_hint"], "Lofiji")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            state = chatgpt.load(path)
            host = state["host"]
            self.assertIn("Connected", action("connect"))
            self.callback.join(timeout=3)
            self.assertEqual(self.params["client_id"], self.client)
            self.assertNotIn("agent_name_hint", self.params)
            self.assertEqual(self.params["ext_agent_host_id"], host)

            state = chatgpt.load(path)
            state["accounts"][0]["expiry"] = 0
            chatgpt.save(path, state)
            self.assertEqual(json.loads(action("models")), [
                {"slug": "example", "name": "Example Model"},
                {"slug": "gpt-6-luna", "name": "Second Option"},
            ])
            self.assertEqual(action("chat", instructions="Planning context",
                                   messages=[{"role": "user", "content": "What next?"}],
                                   model="", timeout=5), "Review your notes.")
            self.expect_effort = True
            self.assertEqual(action("chat", instructions="Planning context",
                                   messages=[{"role": "user", "content": "What next?"}],
                                   model="", timeout=5, thinking_effort="high"), "Review your notes.")
            self.assertEqual(self.fresh, 1)
            self.assertEqual(chatgpt.load(path)["accounts"][0]["refresh"], "rotated-refresh")
            self.assertIn("signed out", action("disconnect"))
            self.assertTrue(self.saw_revocation)
            saved = chatgpt.load(path)["accounts"][0]
            self.assertEqual(saved["client"], self.client)
            self.assertNotIn("access", saved)
            self.assertNotIn("refresh", saved)
            self.assertNotIn("id_token", saved)

    def test_rejects_wrong_nonce_and_unfinished_stream(self):
        with patch.object(chatgpt, "document", self.document):
            reply = self.token_reply("valid")
            with self.assertRaisesRegex(chatgpt.Problem, "validate"):
                chatgpt.identity(reply["id_token"], self.client, "wrong", self.token)
        with patch.object(chatgpt, "request", return_value=io.BytesIO(
                b'data: {"type":"response.output_text.delta","delta":"Partial"}\n\n')):
            with self.assertRaisesRegex(chatgpt.Problem, "before completion"):
                chatgpt.generate(self.token, "context", [], "example", 5)


if __name__ == "__main__":
    unittest.main()
