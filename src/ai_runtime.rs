//! Connect asynchronous provider replies to the conversation and daily guidance.
use crate::{
    ai::{self, Client, Message, Provider},
    app::App,
    planner::Data,
};
use chrono::{Local, NaiveDate};
use std::time::Instant;
#[derive(Clone, Copy)]
enum Target {
    Conversation(u64),
    Daily,
}
struct Active {
    id: u64,
    target: Target,
    settings: ai::Settings,
    data: Data,
    date: NaiveDate,
}
pub struct Runtime {
    client: Client,
    active: Option<Active>,
}
impl Runtime {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            active: None,
        }
    }
    pub fn tick(&mut self, app: &mut App, now: Instant) -> bool {
        let mut dirty = false;
        if let Some(job) = &self.active {
            let changed = job.settings != app.config.ai
                || (job.settings.provider == Provider::Chatgpt && app.chatgpt_busy())
                || match job.target {
                    Target::Conversation(session) => !app
                        .conversation_mut()
                        .is_some_and(|view| view.session == session && view.busy),
                    Target::Daily => false,
                };
            if changed {
                if let Target::Conversation(session) = job.target {
                    if let Some(view) = app
                        .conversation_mut()
                        .filter(|view| view.session == session)
                    {
                        view.cancel_reply();
                    }
                }
                self.client.cancel();
                self.active = None;
                dirty = true;
            }
        }
        if let Some((id, result)) = self.client.poll() {
            if let Some(job) = self.active.take().filter(|j| j.id == id) {
                match job.target {
                    Target::Conversation(session) => {
                        let (who, message) = match result {
                            Ok(text) => (format!("AI · {}", job.settings.provider.label()), text),
                            Err(error) => (
                                "Lofiji".into(),
                                format!(
                                    "AI unavailable: {error}\nLocal next step:\n{}",
                                    app.planner.data.brief(Local::now().date_naive())
                                ),
                            ),
                        };
                        if let Some(view) = app
                            .conversation_mut()
                            .filter(|view| view.session == session)
                        {
                            view.busy = false;
                            view.scroll_back = 0;
                            view.messages.push((who, message));
                            view.trim_history();
                            dirty = true;
                        }
                    }
                    Target::Daily => match result {
                        Ok(text)
                            if app.planner.data == job.data && app.config.ai == job.settings =>
                        {
                            app.ai_daily = job
                                .data
                                .pending(job.date)
                                .first()
                                .and_then(|task| ai::DailyBrief::from_reply(&text, task.id));
                            let message = if app.ai_daily.is_some() {
                                "AI next step ready"
                            } else {
                                "AI reply did not match your next task · showing the local plan"
                            };
                            app.ai_daily_data = Some(job.data);
                            app.ai_daily_settings = Some(job.settings);
                            app.ai_daily_date = Some(job.date);
                            app.message(message, now);
                            dirty = true;
                        }
                        Ok(_) => {
                            app.message("Goals changed · d requests a fresh AI suggestion", now);
                            dirty = true;
                        }
                        Err(error) => {
                            app.message(format!("AI plan unavailable: {error}"), now);
                            dirty = true;
                        }
                    },
                }
            }
        }
        let default_effort = app.config.ai.thinking_effort;
        let chat = app.conversation_mut().and_then(|view| {
            view.pending
                .take()
                .map(|_| (view.session, view.history(), view.effort(default_effort)))
        });
        let date = Local::now().date_naive();
        if let Some((session, messages, effort)) = chat {
            let settings = app.config.ai.clone();
            let data = app.planner.data.clone();
            let id = self.client.start(
                ai::prepare(
                    ai::Settings {
                        thinking_effort: effort,
                        ..settings.clone()
                    },
                    &data,
                    messages,
                    false,
                    date,
                ),
                app.config_path.clone(),
            );
            self.active = Some(Active {
                id,
                target: Target::Conversation(session),
                settings,
                data,
                date,
            });
            dirty = true;
        } else if app.ai_daily_requested {
            app.ai_daily_requested = false;
            if self.active.is_some() {
                app.message("Wait for the current AI reply, then press d", now);
                return true;
            }
            if app.config.ai.provider == Provider::Off {
                app.message("Daily plan uses local rules · s configures AI", now);
                return true;
            }
            if app.planner.error.is_some() {
                app.message("Repair the goals file before requesting AI advice", now);
                return true;
            }
            if app.planner.data.pending(date).is_empty() {
                app.message("No pending tasks · press + to add one", now);
                return true;
            }
            let settings = app.config.ai.clone();
            let data = app.planner.data.clone();
            let messages = vec![Message {
                role: "user".into(),
                content: "Suggest my next step for today from the saved goals and tasks.".into(),
            }];
            let id = self.client.start(
                ai::prepare(settings.clone(), &data, messages, true, date),
                app.config_path.clone(),
            );
            self.active = Some(Active {
                id,
                target: Target::Daily,
                settings,
                data,
                date,
            });
            app.message("Preparing an AI daily suggestion…", now);
            dirty = true;
        }
        dirty
    }
}
