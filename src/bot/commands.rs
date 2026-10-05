use teloxide::utils::command::BotCommands;

/// All bot commands the user can invoke.
#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "Show help. Use /help all for the full command list")]
    Help(String),
    #[command(description = "Start the bot")]
    Start,
    /// Record today's weight: /weight 82.5
    #[command(description = "Record your weight in kg: /weight 82.5")]
    Weight(String),
    /// Show your last recorded weight
    #[command(description = "Show your latest weight entry")]
    LastWeight,
    /// Weight history chart: /weightchart [30|90|all]
    #[command(description = "Weight history chart: /weightchart [30|90|all]")]
    WeightChart(String),
    /// Daily nutrition table: /nutrition [today|YYYY-MM-DD]
    #[command(description = "Daily nutrition table: /nutrition [today|yesterday|YYYY-MM-DD]")]
    Nutrition(String),
    /// Log a training session: /training Ran 5km
    #[command(description = "Log a training session: /training Ran 5km")]
    Training(String),
    /// Show recent training sessions: /lasttraining [n]
    #[command(description = "Show your last training entries: /lasttraining [n]")]
    LastTraining(String),
    /// Show one training session as a full card: /showtraining <id>
    #[command(description = "Show a training session card: /showtraining <id>")]
    ShowTraining(String),
    /// Update a training entry: /updatetraining <id> <new description>
    #[command(description = "Update a training entry: /updatetraining <id> <description>")]
    UpdateTraining(String),
    /// Delete a training entry: /deltraining <id>
    #[command(description = "Delete a training entry: /deltraining <id>")]
    DelTraining(String),
    /// Add a user by Telegram user id or username: /user add <chat_id|@username>
    #[command(
        description = "Add a user by Telegram user id or username: /user add <chat_id|@username>"
    )]
    UserAdd(String),
    /// Remove a user by Telegram user id or username: /user remove <chat_id|@username>
    #[command(
        description = "Remove a user by Telegram user id or username: /user remove <chat_id|@username>"
    )]
    UserRemove(String),
    /// List all users: /user list
    #[command(description = "List all users: /user list")]
    UserList,
    /// Set a user's role: /user setrole <chat_id|@username> <role>
    #[command(description = "Set a user's role: /user setrole <chat_id|@username> <role>")]
    UserSetRole(String),
    /// Ask the AI about your data: /ask How am I doing?
    #[command(description = "Ask the AI about your data: /ask How am I doing?")]
    Ask(String),
    /// Set a goal: /setgoal weight 75.0 or /setgoal training 3
    #[command(description = "Set a goal: /setgoal weight 75.0 or /setgoal training 3")]
    SetGoal(String),
    /// Show your active goals and progress
    #[command(description = "Show your active goals and progress")]
    Goals,
    /// Delete a goal by id: /delgoal 1
    #[command(description = "Delete a goal by id: /delgoal 1")]
    DelGoal(String),
    /// Create a task: /task Buy milk
    #[command(description = "Create a new task: /task Buy milk")]
    Task(String),
    /// List your tasks: /tasks
    #[command(description = "List your pending tasks")]
    Tasks,
    /// Mark a task as done: /done 1
    #[command(description = "Mark a task as done: /done <task_id>")]
    DoneTask(String),
    /// Cancel a task: /canceltask 1
    #[command(description = "Cancel a task: /canceltask <task_id>")]
    CancelTask(String),
    /// Delete a task: /deltask 1
    #[command(description = "Delete a task permanently: /deltask <task_id>")]
    Deltask(String),
    /// Set a reminder: /remind tomorrow 9am Buy milk
    #[command(description = "Set a reminder: /remind <when> <message>")]
    Remind(String),
    /// List pending reminders: /reminders
    #[command(description = "List your pending reminders")]
    Reminders,
    /// Cancel a reminder: /cancelremind 1
    #[command(description = "Cancel a reminder by ID: /cancelremind <reminder_id>")]
    CancelRemind(String),
    /// Set your profile description: /setprofile I'm training for a marathon
    #[command(description = "Set your AI profile: /setprofile <description>")]
    SetProfile(String),
    /// Show your current profile description
    #[command(description = "Show your current AI profile")]
    Profile,
    /// Propose a feature: /propose Dark mode for the bot
    #[command(description = "Propose a new feature: /propose Dark mode")]
    Propose(String),
    /// List feature proposals (admin sees all, users see their own)
    #[command(description = "List feature proposals")]
    Proposals,
    /// Delete a proposal: /delproposal 1
    #[command(description = "Delete a proposal: /delproposal <id>")]
    DelProposal(String),
    /// Manage daily recap: /dailyrecap [on|off|at HH:MM]
    #[command(description = "Manage daily recap: /dailyrecap [on|off|at HH:MM]")]
    DailyRecap(String),
    /// Show estimated AI spend for a user (admin only): /usercost <chat_id|@username> [7d|30d|all]
    #[command(
        description = "Show estimated AI spend for a user (admin): /usercost <chat_id|@username> [7d|30d|all]"
    )]
    UserCost(String),
    /// Show top users by estimated AI spend (admin only): /topusage [limit]
    #[command(description = "Show top users by estimated AI spend (admin): /topusage [limit]")]
    TopUsage(String),
    /// Group training graph: /monthlygraph last | 4w | 3m | [on|off|now] (admin)
    #[command(
        description = "Group training graph: /monthlygraph last|4w|3m (anyone) or on|off|now (admin)"
    )]
    MonthlyGraph(String),
    /// Manage inactivity nudges: /nudges [on|off|at N]
    #[command(description = "Manage inactivity nudges: /nudges [on|off|at N]")]
    Nudges(String),
    /// Show your current weight and training streaks
    #[command(description = "Show your current weight and training streaks")]
    Streak,
    /// Set your timezone: /timezone Europe/Paris
    #[command(description = "Set your timezone: /timezone <IANA timezone>")]
    Timezone(String),
    /// Set your bot language: /lang en|fr
    #[command(description = "Set your bot language: /lang en|fr")]
    Lang(String),
    /// Set the sport reaction emoji for this chat (admin only): /setsportemoji 🏆
    #[command(
        description = "Set sport reaction emoji for this chat (admin): /setsportemoji <emoji>"
    )]
    SetSportEmoji(String),
    /// Enable web search for a user (admin only): /enablewebsearch <chat_id|@username>
    #[command(
        description = "Enable web search for a user (admin): /enablewebsearch <chat_id|@username>"
    )]
    EnableWebSearch(String),
    /// Disable web search for a user (admin only): /disablewebsearch <chat_id|@username>
    #[command(
        description = "Disable web search for a user (admin): /disablewebsearch <chat_id|@username>"
    )]
    DisableWebSearch(String),
    /// Show RAG status and chunk count
    #[command(description = "Show RAG status and chunk count")]
    Rag,
    /// Rebuild RAG embeddings from conversation history (admin only)
    #[command(description = "Rebuild RAG embeddings from history (admin): /ragrebuild")]
    RagRebuild,
}

impl Command {
    /// Slash-command name without arguments, for usage logs.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Help(_) => "help",
            Self::Start => "start",
            Self::Weight(_) => "weight",
            Self::LastWeight => "lastweight",
            Self::WeightChart(_) => "weightchart",
            Self::Nutrition(_) => "nutrition",
            Self::Training(_) => "training",
            Self::LastTraining(_) => "lasttraining",
            Self::ShowTraining(_) => "showtraining",
            Self::UpdateTraining(_) => "updatetraining",
            Self::DelTraining(_) => "deltraining",
            Self::UserAdd(_) => "useradd",
            Self::UserRemove(_) => "userremove",
            Self::UserList => "userlist",
            Self::UserSetRole(_) => "usersetrole",
            Self::Ask(_) => "ask",
            Self::SetGoal(_) => "setgoal",
            Self::Goals => "goals",
            Self::DelGoal(_) => "delgoal",
            Self::Task(_) => "task",
            Self::Tasks => "tasks",
            Self::DoneTask(_) => "done",
            Self::CancelTask(_) => "canceltask",
            Self::Deltask(_) => "deltask",
            Self::Remind(_) => "remind",
            Self::Reminders => "reminders",
            Self::CancelRemind(_) => "cancelremind",
            Self::SetProfile(_) => "setprofile",
            Self::Profile => "profile",
            Self::Propose(_) => "propose",
            Self::Proposals => "proposals",
            Self::DelProposal(_) => "delproposal",
            Self::DailyRecap(_) => "dailyrecap",
            Self::UserCost(_) => "usercost",
            Self::TopUsage(_) => "topusage",
            Self::MonthlyGraph(_) => "monthlygraph",
            Self::Nudges(_) => "nudges",
            Self::Streak => "streak",
            Self::Timezone(_) => "timezone",
            Self::Lang(_) => "lang",
            Self::SetSportEmoji(_) => "setsportemoji",
            Self::EnableWebSearch(_) => "enablewebsearch",
            Self::DisableWebSearch(_) => "disablewebsearch",
            Self::Rag => "rag",
            Self::RagRebuild => "ragrebuild",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn slug_omits_arguments() {
        assert_eq!(Command::Start.slug(), "start");
        assert_eq!(Command::Weight("82.5".into()).slug(), "weight");
        assert_eq!(Command::Ask("how am I doing?".into()).slug(), "ask");
        assert_eq!(Command::Help("all".into()).slug(), "help");
    }
}
