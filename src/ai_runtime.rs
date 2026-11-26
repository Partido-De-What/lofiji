//! Route independent provider replies back to the workspace.
use crate::{
    ai::{self, Client, Message, Provider},
    app::App,
    planner::Data,
};
use chrono::{Local, NaiveDate};
use std::time::Instant;

struct Conversation {
    session: u64,
    settings: ai::Settings,
    account_revision: u64,
}
struct Daily {
    settings: ai::Settings,
    account_revision: u64,
    data: Data,
    date: NaiveDate,
}
pub struct Runtime {
    chat_client: Client,
    daily_client: Client,
    conversation: Option<Conversation>,
    daily: Option<Daily>,
}
impl Runtime {
    pub fn new() -> Self {
        Self {
            chat_client: Client::new(),
            daily_client: Client::new(),
            conversation: None,
            daily: None,
        }
    }
    pub fn tick(&mut self, app: &mut App, now: Instant) -> bool {
        let account_changed = app.tick_chatgpt();
        let date = Local::now().date_naive();
        let conversation_changed = self.tick_conversation(app, date);
        let daily_changed = self.tick_daily(app, date, now);
        account_changed || conversation_changed || daily_changed
    }

    fn tick_conversation(&mut self, app: &mut App, date: NaiveDate) -> bool {
        let settings = app.config.ai.clone();
        let account_revision = app.chatgpt_revision;
        let mut dirty = false;
        if let Some(job) = &self.conversation {
            let view = app.conversation_mut().filter(|v| v.session == job.session);
            if job.settings != settings
                || (settings.provider == Provider::Chatgpt
                    && job.account_revision != account_revision)
                || view.as_ref().map_or(true, |v| !v.busy)
            {
                if let Some(view) = view {
                    view.cancel_reply();
                }
                self.chat_client.cancel();
                self.conversation = None;
                dirty = true;
            }
        }
        if let Some((_, result)) = self.chat_client.poll() {
            if let Some(job) = self.conversation.take() {
                let (who, message) = match result {
                    Ok(text) => (format!("AI · {}", job.settings.provider.label()), text),
                    Err(error) => (
                        "Lofiji".into(),
                        format!(
                            "AI unavailable: {error}\nLocal next step:\n{}",
                            app.planner.data.brief(date)
                        ),
                    ),
                };
                if let Some(view) = app.conversation_mut().filter(|v| v.session == job.session) {
                    view.busy = false;
                    view.scroll_back = 0;
                    view.messages.push((who, message));
                    view.trim_history();
                    dirty = true;
                }
            }
        }
        if let Some((session, messages, effort)) = app.conversation_mut().and_then(|view| {
            view.pending.take().map(|_| {
                (
                    view.session,
                    view.history(),
                    view.effort(settings.thinking_effort),
                )
            })
        }) {
            let request = ai::prepare(
                ai::Settings {
                    thinking_effort: effort,
                    ..settings.clone()
                },
                &app.planner.data,
                messages,
                false,
                date,
            );
            self.chat_client.start(request, app.config_path.clone());
            self.conversation = Some(Conversation {
                session,
                settings,
                account_revision,
            });
            dirty = true;
        }
        dirty
    }

    fn tick_daily(&mut self, app: &mut App, date: NaiveDate, now: Instant) -> bool {
        let mut dirty = false;
        if self.daily.as_ref().is_some_and(|job| {
            job.settings != app.config.ai
                || job.date != date
                || (job.settings.provider == Provider::Chatgpt
                    && job.account_revision != app.chatgpt_revision)
        }) {
            self.daily_client.cancel();
            self.daily = None;
            dirty = true;
        }
        if let Some((_, result)) = self.daily_client.poll() {
            if let Some(job) = self.daily.take() {
                let message = match result {
                    Ok(text) if app.planner.data == job.data => {
                        app.ai_daily = job
                            .data
                            .pending(job.date)
                            .first()
                            .and_then(|task| ai::DailyBrief::from_reply(&text, task.id));
                        app.ai_daily_data = Some(job.data);
                        app.ai_daily_settings = Some(job.settings);
                        app.ai_daily_date = Some(job.date);
                        if app.ai_daily.is_some() {
                            "AI next step ready".into()
                        } else {
                            "AI reply did not match your next task · showing the local plan".into()
                        }
                    }
                    Ok(_) => "Goals changed · d requests a fresh AI suggestion".into(),
                    Err(error) => format!("AI plan unavailable: {error}"),
                };
                // The result is visible on the dashboard even while another view owns the footer.
                app.message(message, now);
                dirty = true;
            }
        }
        if std::mem::take(&mut app.ai_daily_requested) {
            let notice = if self.daily.is_some() {
                "An AI daily suggestion is already in progress"
            } else if app.config.ai.provider == Provider::Off {
                "Daily plan uses local rules · s configures AI"
            } else if app.planner.error.is_some() {
                "Repair the goals file before requesting AI advice"
            } else if app.planner.data.pending(date).is_empty() {
                "No pending tasks · press + to add one"
            } else {
                let settings = app.config.ai.clone();
                let data = app.planner.data.clone();
                let messages = vec![Message {
                    role: "user".into(),
                    content: "Suggest my next step for today from the saved goals and tasks."
                        .into(),
                }];
                self.daily_client.start(
                    ai::prepare(settings.clone(), &data, messages, true, date),
                    app.config_path.clone(),
                );
                self.daily = Some(Daily {
                    settings,
                    data,
                    date,
                    account_revision: app.chatgpt_revision,
                });
                "Preparing an AI daily suggestion…"
            };
            app.message(notice, now);
            dirty = true;
        }
        dirty
    }
}
