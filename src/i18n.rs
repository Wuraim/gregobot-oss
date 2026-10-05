use crate::db::users::Role;

fn str_lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|&s| s.to_string()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    pub fn from_iso_code(code: Option<&str>) -> Self {
        match code {
            Some("fr") => Lang::Fr,
            _ => Lang::En,
        }
    }
}

pub struct UserCostReport<'a> {
    pub telegram_user_id: i64,
    pub window: &'a str,
    pub request_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub cache_usd_saved: f64,
    pub estimated_cost: f64,
    pub model_lines: &'a [String],
    pub kind_lines: &'a [String],
}

pub struct Msg {
    lang: Lang,
}

impl Msg {
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    // ── Start ───────────────────────────────────────────────────────────────
    pub fn start(&self, telegram_user_id: i64, username_display: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "Hey! I'm your life companion — I help with fitness, tasks, reminders and more.\n\
                 Your Telegram user ID: `{}`{}\n\n\
                 Please set your preferences:\n\
                 • /timezone Europe/Paris — your timezone (IANA name)\n\
                 • /lang fr — or /lang en — the language I should use with you\n\n\
                 To get access, please contact the admin.",
                telegram_user_id, username_display
            ),
            Lang::Fr => format!(
                "Salut ! Je suis ton compagnon de vie — je t'aide avec le fitness, les tâches, les rappels et plus encore.\n\
                 Ton ID utilisateur Telegram : `{}`{}\n\n\
                 Merci de renseigner :\n\
                 • /timezone Europe/Paris — ton fuseau horaire (nom IANA)\n\
                 • /lang fr — ou /lang en — la langue dans laquelle je te parle\n\n\
                 Pour obtenir l'accès, contacte l'administrateur.",
                telegram_user_id, username_display
            ),
        }
    }

    pub fn timezone_current(&self, timezone: &str) -> String {
        match self.lang {
            Lang::En => format!("Your current timezone: {timezone}."),
            Lang::Fr => format!("Votre fuseau horaire actuel : {timezone}."),
        }
    }

    pub fn timezone_set(&self, timezone: &str) -> String {
        match self.lang {
            Lang::En => format!("Timezone set to: {timezone}."),
            Lang::Fr => format!("Fuseau horaire défini sur : {timezone}."),
        }
    }

    pub fn timezone_invalid(&self, input: &str) -> String {
        match self.lang {
            Lang::En => format!("Invalid timezone: {input}. Use an IANA name like Europe/Paris."),
            Lang::Fr => format!(
                "Fuseau horaire invalide : {input}. Utilisez un format IANA comme Europe/Paris."
            ),
        }
    }

    pub fn timezone_save_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to save timezone.",
            Lang::Fr => "Erreur lors de l'enregistrement du fuseau horaire.",
        }
    }

    pub fn lang_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /lang en — or /lang fr",
            Lang::Fr => "Usage : /lang en — ou /lang fr",
        }
    }

    pub fn lang_current(&self, code: &str) -> String {
        match self.lang {
            Lang::En => format!("Your current bot language: {code}."),
            Lang::Fr => format!("Votre langue actuelle du bot : {code}."),
        }
    }

    pub fn lang_set(&self, code: &str) -> String {
        match self.lang {
            Lang::En => format!("Bot language set to: {code}."),
            Lang::Fr => format!("Langue du bot définie sur : {code}."),
        }
    }

    pub fn lang_save_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to save language preference.",
            Lang::Fr => "Échec de l'enregistrement de la langue.",
        }
    }

    // ── Auth / General ──────────────────────────────────────────────────────
    pub fn not_authorized(&self) -> &'static str {
        match self.lang {
            Lang::En => "You are not authorized to use this bot.",
            Lang::Fr => "Vous n'êtes pas autorisé à utiliser ce bot.",
        }
    }

    pub fn not_authorized_pending(&self) -> &'static str {
        match self.lang {
            Lang::En => "You are not authorized to use this bot. Contact the admin to get access.",
            Lang::Fr => {
                "Vous n'êtes pas autorisé à utiliser ce bot. Contactez l'administrateur pour obtenir l'accès."
            }
        }
    }

    pub fn internal_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "An internal error occurred. Please try again later.",
            Lang::Fr => "Une erreur interne s'est produite. Veuillez réessayer plus tard.",
        }
    }

    // ── User Management ───────────────────────────────────────────────────────
    pub fn not_admin(&self) -> &'static str {
        match self.lang {
            Lang::En => "You are not authorized to manage users.",
            Lang::Fr => "Vous n'êtes pas autorisé à gérer les utilisateurs.",
        }
    }

    pub fn user_not_found_start(&self) -> &'static str {
        match self.lang {
            Lang::En => "User not found. Please ensure they have sent /start to the bot.",
            Lang::Fr => "Utilisateur introuvable. Assurez-vous qu'il a envoyé /start au bot.",
        }
    }

    pub fn user_not_found(&self) -> &'static str {
        match self.lang {
            Lang::En => "User not found.",
            Lang::Fr => "Utilisateur introuvable.",
        }
    }

    pub fn db_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Database error.",
            Lang::Fr => "Erreur de base de données.",
        }
    }

    pub fn user_add_already_registered(&self, telegram_user_id: i64, role: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "User {} is already registered with role '{}'.",
                telegram_user_id, role
            ),
            Lang::Fr => format!(
                "L'utilisateur {} est déjà enregistré avec le rôle '{}'.",
                telegram_user_id, role
            ),
        }
    }

    pub fn user_add_success(&self, telegram_user_id: i64, id: i64) -> String {
        match self.lang {
            Lang::En => format!("User {} (id: {}) added as sportive.", telegram_user_id, id),
            Lang::Fr => format!(
                "L'utilisateur {} (id: {}) a été ajouté en tant que sportif.",
                telegram_user_id, id
            ),
        }
    }

    pub fn user_add_failed_update(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update user role.",
            Lang::Fr => "Échec de la mise à jour du rôle de l'utilisateur.",
        }
    }

    pub fn user_add_failed(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to add user. Please try again.",
            Lang::Fr => "Échec de l'ajout de l'utilisateur. Veuillez réessayer.",
        }
    }

    pub fn cannot_remove_self(&self) -> &'static str {
        match self.lang {
            Lang::En => "You cannot remove yourself via this command.",
            Lang::Fr => "Vous ne pouvez pas vous supprimer via cette commande.",
        }
    }

    pub fn cannot_remove_last_admin(&self) -> &'static str {
        match self.lang {
            Lang::En => "Cannot remove the last admin user.",
            Lang::Fr => "Impossible de supprimer le dernier administrateur.",
        }
    }

    pub fn user_remove_success(&self, telegram_user_id: i64, id: i64) -> String {
        match self.lang {
            Lang::En => format!("User {} (id: {}) removed.", telegram_user_id, id),
            Lang::Fr => format!(
                "L'utilisateur {} (id: {}) a été supprimé.",
                telegram_user_id, id
            ),
        }
    }

    pub fn user_remove_not_found(&self, telegram_user_id: i64) -> String {
        match self.lang {
            Lang::En => format!("User {} not found.", telegram_user_id),
            Lang::Fr => format!("L'utilisateur {} introuvable.", telegram_user_id),
        }
    }

    pub fn user_remove_failed(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to remove user. Please try again.",
            Lang::Fr => "Échec de la suppression de l'utilisateur. Veuillez réessayer.",
        }
    }

    pub fn user_setrole_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /user setrole <chat_id|@username> <role>",
            Lang::Fr => "Usage : /user setrole <chat_id|@username> <rôle>",
        }
    }

    pub fn unknown_role(&self) -> &'static str {
        match self.lang {
            Lang::En => "Unknown role. Valid roles: admin, sportive, pending.",
            Lang::Fr => "Rôle inconnu. Rôles valides : admin, sportive, pending.",
        }
    }

    pub fn cannot_demote_last_admin(&self) -> &'static str {
        match self.lang {
            Lang::En => "Cannot demote the last admin user.",
            Lang::Fr => "Impossible de rétrograder le dernier administrateur.",
        }
    }

    pub fn user_setrole_success(&self, telegram_user_id: i64, id: i64, role: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "User {} (id: {}) set to role '{}'.",
                telegram_user_id, id, role
            ),
            Lang::Fr => format!(
                "Rôle de l'utilisateur {} (id: {}) défini sur '{}'.",
                telegram_user_id, id, role
            ),
        }
    }

    pub fn user_setrole_not_updated(&self) -> &'static str {
        match self.lang {
            Lang::En => "No user updated (not found).",
            Lang::Fr => "Aucun utilisateur mis à jour (introuvable).",
        }
    }

    pub fn user_setrole_db_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Database error during role set.",
            Lang::Fr => "Erreur de base de données lors de la définition du rôle.",
        }
    }

    pub fn user_setrole_lookup_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Database error during role lookup.",
            Lang::Fr => "Erreur de base de données lors de la recherche du rôle.",
        }
    }

    pub fn no_users_registered(&self) -> &'static str {
        match self.lang {
            Lang::En => "No users registered.",
            Lang::Fr => "Aucun utilisateur enregistré.",
        }
    }

    pub fn registered_users_header(&self) -> &'static str {
        match self.lang {
            Lang::En => "Registered users:\n",
            Lang::Fr => "Utilisateurs enregistrés :\n",
        }
    }

    pub fn user_list_entry(
        &self,
        id: i64,
        telegram_user_id: i64,
        role: &str,
        username_display: &str,
    ) -> String {
        match self.lang {
            Lang::En => format!(
                "• id: {} | tg: {} | role: {}{}\n",
                id, telegram_user_id, role, username_display
            ),
            Lang::Fr => format!(
                "• id: {} | tg: {} | rôle: {}{}\n",
                id, telegram_user_id, role, username_display
            ),
        }
    }

    // ── Help ────────────────────────────────────────────────────────────────
    pub fn help_unavailable(&self) -> &'static str {
        match self.lang {
            Lang::En => "Help unavailable due to an internal error.",
            Lang::Fr => "Aide indisponible en raison d'une erreur interne.",
        }
    }

    pub fn unregistered_help(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "/start - Start the bot and see your user ID\n\nTo access more commands, please contact the admin."
            }
            Lang::Fr => {
                "/start - Démarrer le bot et voir votre ID utilisateur\n\nPour accéder à plus de commandes, contactez l'administrateur."
            }
        }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    // ── Inline Keyboard Buttons ─────────────────────────────────────────────
    pub fn btn_record_weight(&self) -> &'static str {
        match self.lang {
            Lang::En => "Record Weight",
            Lang::Fr => "Enregistrer le poids",
        }
    }

    pub fn btn_log_training(&self) -> &'static str {
        match self.lang {
            Lang::En => "Log Training",
            Lang::Fr => "Enregistrer entraînement",
        }
    }

    pub fn btn_ask_ai(&self) -> &'static str {
        match self.lang {
            Lang::En => "Ask AI",
            Lang::Fr => "Demander à l'IA",
        }
    }

    pub fn btn_my_stats(&self) -> &'static str {
        match self.lang {
            Lang::En => "My Stats",
            Lang::Fr => "Mes stats",
        }
    }

    pub fn btn_custom(&self) -> &'static str {
        match self.lang {
            Lang::En => "Custom...",
            Lang::Fr => "Personnalisé...",
        }
    }

    pub fn btn_last_weight(&self) -> &'static str {
        match self.lang {
            Lang::En => "Last Weight",
            Lang::Fr => "Dernier poids",
        }
    }

    pub fn btn_weight_chart(&self) -> &'static str {
        match self.lang {
            Lang::En => "Weight Chart",
            Lang::Fr => "Courbe de poids",
        }
    }

    pub fn btn_last_training(&self) -> &'static str {
        match self.lang {
            Lang::En => "Last Training",
            Lang::Fr => "Dernier entraînement",
        }
    }

    pub fn btn_back(&self) -> &'static str {
        match self.lang {
            Lang::En => "← Back",
            Lang::Fr => "← Retour",
        }
    }

    pub fn btn_ran_5km(&self) -> &'static str {
        match self.lang {
            Lang::En => "🏃 Ran 5km",
            Lang::Fr => "🏃 Course 5km",
        }
    }

    pub fn btn_gym_session(&self) -> &'static str {
        match self.lang {
            Lang::En => "🏋️ Gym Session",
            Lang::Fr => "🏋️ Muscu",
        }
    }

    pub fn btn_cycling(&self) -> &'static str {
        match self.lang {
            Lang::En => "🚴 Cycling",
            Lang::Fr => "🚴 Vélo",
        }
    }

    // ── Menu Texts ────────────────────────────────────────────────────────────
    pub fn menu_main_text(&self) -> &'static str {
        match self.lang {
            Lang::En => "What would you like to do?",
            Lang::Fr => "Que souhaitez-vous faire ?",
        }
    }

    pub fn menu_weight_text(&self) -> &'static str {
        match self.lang {
            Lang::En => "Select your weight or choose Custom:",
            Lang::Fr => "Sélectionnez votre poids ou choisissez Personnalisé :",
        }
    }

    pub fn menu_training_text(&self) -> &'static str {
        match self.lang {
            Lang::En => "Select a training activity or choose Custom:",
            Lang::Fr => "Sélectionnez une activité ou choisissez Personnalisé :",
        }
    }

    pub fn menu_stats_text(&self) -> &'static str {
        match self.lang {
            Lang::En => "Your stats:",
            Lang::Fr => "Vos statistiques :",
        }
    }

    pub fn weight_custom_prompt(&self) -> &'static str {
        match self.lang {
            Lang::En => "Please send your weight: /weight 82.5",
            Lang::Fr => "Veuillez envoyer votre poids : /weight 82.5",
        }
    }

    pub fn training_custom_prompt(&self) -> &'static str {
        match self.lang {
            Lang::En => "Please describe your training: /training Ran 5km",
            Lang::Fr => "Veuillez décrire votre entraînement : /training Course 5km",
        }
    }

    // ── Goals Keyboard Buttons ──────────────────────────────────────────────
    pub fn btn_goals(&self) -> &'static str {
        match self.lang {
            Lang::En => "Goals",
            Lang::Fr => "Objectifs",
        }
    }

    pub fn btn_set_weight_goal(&self) -> &'static str {
        match self.lang {
            Lang::En => "Set Weight Goal",
            Lang::Fr => "Objectif de poids",
        }
    }

    pub fn btn_set_training_goal(&self) -> &'static str {
        match self.lang {
            Lang::En => "Set Training Goal",
            Lang::Fr => "Objectif entraînement",
        }
    }

    pub fn btn_my_goals(&self) -> &'static str {
        match self.lang {
            Lang::En => "My Goals",
            Lang::Fr => "Mes objectifs",
        }
    }

    pub fn menu_goals_text(&self) -> &'static str {
        match self.lang {
            Lang::En => "Your goals:",
            Lang::Fr => "Vos objectifs :",
        }
    }

    pub fn goal_weight_set_prompt(&self) -> &'static str {
        match self.lang {
            Lang::En => "To set a weight goal, type: /setgoal weight 75.0",
            Lang::Fr => "Pour définir un objectif de poids, tapez : /setgoal weight 75.0",
        }
    }

    pub fn goal_training_set_prompt(&self) -> &'static str {
        match self.lang {
            Lang::En => "To set a training goal, type: /setgoal training 3",
            Lang::Fr => "Pour définir un objectif d'entraînement, tapez : /setgoal training 3",
        }
    }

    // ── Goals Messages ──────────────────────────────────────────────────────
    pub fn goal_set_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /setgoal weight <kg> or /setgoal training <sessions_per_week>",
            Lang::Fr => "Usage : /setgoal weight <kg> ou /setgoal training <séances_par_semaine>",
        }
    }

    pub fn goal_set_invalid_type(&self) -> &'static str {
        match self.lang {
            Lang::En => "Invalid goal type. Use 'weight' or 'training'.",
            Lang::Fr => "Type d'objectif invalide. Utilisez 'weight' ou 'training'.",
        }
    }

    pub fn goal_set_invalid_value(&self) -> &'static str {
        match self.lang {
            Lang::En => "Invalid value. Provide a number (e.g. /setgoal weight 75.0).",
            Lang::Fr => "Valeur invalide. Fournissez un nombre (ex. /setgoal weight 75.0).",
        }
    }

    pub fn goal_set_success_weight(&self, target: f64) -> String {
        match self.lang {
            Lang::En => format!("Goal set: weight target {:.1} kg.", target),
            Lang::Fr => format!("Objectif défini : poids cible {:.1} kg.", target),
        }
    }

    pub fn goal_set_success_training(&self, target: f64) -> String {
        match self.lang {
            Lang::En => format!("Goal set: training target {:.0} sessions per week.", target),
            Lang::Fr => format!(
                "Objectif défini : entraînement cible {:.0} séances par semaine.",
                target
            ),
        }
    }

    pub fn goal_delete_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /delgoal <goal_id>",
            Lang::Fr => "Usage : /delgoal <objectif_id>",
        }
    }

    pub fn goal_delete_success(&self) -> &'static str {
        match self.lang {
            Lang::En => "Goal deleted.",
            Lang::Fr => "Objectif supprimé.",
        }
    }

    pub fn goal_delete_not_found(&self) -> &'static str {
        match self.lang {
            Lang::En => "Goal not found.",
            Lang::Fr => "Objectif introuvable.",
        }
    }

    pub fn goal_no_goals(&self) -> &'static str {
        match self.lang {
            Lang::En => "No active goals. Use /setgoal to set one.",
            Lang::Fr => "Aucun objectif actif. Utilisez /setgoal pour en définir un.",
        }
    }

    pub fn goal_progress_header(&self) -> &'static str {
        match self.lang {
            Lang::En => "Your active goals:\n",
            Lang::Fr => "Vos objectifs actifs :\n",
        }
    }

    pub fn goal_progress_entry(&self, id: i64, description: &str) -> String {
        match self.lang {
            Lang::En => format!("• #{}: {}\n", id, description),
            Lang::Fr => format!("• #{} : {}\n", id, description),
        }
    }

    pub fn goal_progress_weight(&self, current: Option<f64>, target: f64) -> String {
        match self.lang {
            Lang::En => match current {
                Some(c) if c <= target => format!("Weight: {:.1} / {:.1} kg ✓ achieved", c, target),
                Some(c) => format!(
                    "Weight: {:.1} / {:.1} kg ({:.1} kg to go)",
                    c,
                    target,
                    c - target
                ),
                None => format!("Weight: — / {:.1} kg (no entries yet)", target),
            },
            Lang::Fr => match current {
                Some(c) if c <= target => format!("Poids : {:.1} / {:.1} kg ✓ atteint", c, target),
                Some(c) => format!(
                    "Poids : {:.1} / {:.1} kg ({:.1} kg restant)",
                    c,
                    target,
                    c - target
                ),
                None => format!("Poids : — / {:.1} kg (aucune entrée)", target),
            },
        }
    }

    pub fn goal_progress_training(&self, current: f64, target: f64) -> String {
        match self.lang {
            Lang::En => {
                if current >= target {
                    format!(
                        "Training: {:.0} / {:.0} sessions this week ✓ achieved",
                        current, target
                    )
                } else {
                    format!(
                        "Training: {:.0} / {:.0} sessions this week ({:.0} to go)",
                        current,
                        target,
                        target - current
                    )
                }
            }
            Lang::Fr => {
                if current >= target {
                    format!(
                        "Entraînement : {:.0} / {:.0} séances cette semaine ✓ atteint",
                        current, target
                    )
                } else {
                    format!(
                        "Entraînement : {:.0} / {:.0} séances cette semaine ({:.0} restant)",
                        current,
                        target,
                        target - current
                    )
                }
            }
        }
    }

    pub fn goal_achieved_weight(&self, target: f64) -> String {
        match self.lang {
            Lang::En => format!("🎉 Goal achieved: weight target {:.1} kg reached!", target),
            Lang::Fr => format!(
                "🎉 Objectif atteint : poids cible {:.1} kg atteint !",
                target
            ),
        }
    }

    pub fn goal_achieved_training(&self, target: f64) -> String {
        match self.lang {
            Lang::En => format!(
                "🎉 Goal achieved: {:.0} training sessions this week!",
                target
            ),
            Lang::Fr => format!(
                "🎉 Objectif atteint : {:.0} séances d'entraînement cette semaine !",
                target
            ),
        }
    }

    pub fn help_commands_text(&self, role: &Role, full: bool) -> String {
        match self.lang {
            Lang::En => self.help_en(role, full),
            Lang::Fr => self.help_fr(role, full),
        }
    }

    fn help_en(&self, role: &Role, full: bool) -> String {
        let mut lines: Vec<String> = Vec::new();

        let is_admin = matches!(role, Role::Admin);

        lines.push("📋 Quick help — type /help all for the full command list:".to_string());
        lines.push("".to_string());
        lines.push("Body & Fitness".to_string());
        lines.push("  /weight <kg>        — Record your weight".to_string());
        lines.push("  /lastweight         — Show your latest weight".to_string());
        lines.push("  /weightchart [30|90|all] — Weight history chart".to_string());
        lines.push("  /nutrition [date]   — Daily nutrition table".to_string());
        lines.push("  /training <desc>    — Log a training session".to_string());
        lines.push("  /lasttraining [n]   — Show your last sessions (default 5)".to_string());
        lines.push("  /showtraining <id>  — Show a session card".to_string());
        lines.push("  /updatetraining <id> <desc> — Update a session".to_string());
        lines.push("  /deltraining <id>   — Delete a session".to_string());
        lines.push("  /streak             — Show your current streaks".to_string());
        lines.push("  /monthlygraph last|4w|3m  — Group training chart".to_string());

        lines.push("".to_string());
        lines.push("AI".to_string());
        lines.push("  /ask <question>     — Ask the AI about your data".to_string());

        lines.push("".to_string());
        lines.push("Tasks".to_string());
        lines.push("  /task <title>       — Create a task".to_string());
        lines.push("  /tasks              — List your tasks".to_string());
        lines.push("  /done <id>          — Mark a task done".to_string());

        lines.push("".to_string());
        lines.push("Reminders".to_string());
        lines.push("  /remind <when> ...  — Set a reminder".to_string());

        lines.push("".to_string());
        lines.push("Settings".to_string());
        lines.push("  /timezone <IANA>    — Set your timezone".to_string());
        lines.push("  /lang en|fr         — Set bot language".to_string());
        lines.push("  /setprofile <desc>  — Set your AI profile".to_string());
        lines.push("  /dailyrecap [on|off|at HH:MM]".to_string());
        lines.push("  /nudges [on|off|at N]".to_string());

        if is_admin {
            lines.push("".to_string());
            lines.push("Admin".to_string());
            lines.push("  /useradd <id|@user> — Add a user".to_string());
            lines.push("  /userlist           — List users".to_string());
            lines.push("  /topusage [limit]   — Top AI spend".to_string());
            lines.push("  /setsportemoji <emoji>".to_string());
            lines.push("  /monthlygraph [on|off|now]".to_string());
        }

        lines.push("".to_string());
        lines.push("Type /help all to see every command by category.".to_string());

        if !full {
            return lines.join("\n");
        }

        let mut full_lines = str_lines(&[
            "📖 Full command list:",
            "",
            "Body & Fitness",
            "  /weight <kg>              — Record your weight in kg",
            "  /lastweight               — Show your latest weight entry",
            "  /weightchart [30|90|all]  — Weight history chart",
            "  /nutrition [today|YYYY-MM-DD] — Daily nutrition table (private)",
            "  /training <desc>          — Log a training session",
            "  /lasttraining [n]         — Show your last n training entries (default 5)",
            "  /showtraining <id>         — Show a full session card (admin: any)",
            "  /updatetraining <id> <desc> — Update your training (admin: any)",
            "  /deltraining <id>         — Delete your training (admin: any)",
            "  /streak                   — Show your current weight and training streaks",
            "  /monthlygraph last|4w|3m  — Group training chart (calendar month, weeks, or months)",
            "",
            "Goals",
            "  /setgoal <type> <value>   — Set a goal (weight 75.0 or training 3)",
            "  /goals                    — Show active goals and progress",
            "  /delgoal <id>             — Delete a goal",
            "",
            "Tasks",
            "  /task <title>             — Create a new task",
            "  /tasks                    — List your tasks",
            "  /done <id>                — Mark a task as done",
            "  /canceltask <id>          — Cancel a task",
            "  /deltask <id>             — Delete a task permanently",
            "",
            "Reminders",
            "  /remind <when> <msg>      — Set a one-time or recurring reminder",
            "  /reminders                — List pending reminders",
            "  /cancelremind <id>        — Cancel a reminder",
            "",
            "AI & Settings",
            "  /ask <question>           — Ask the AI about your data",
            "  /timezone <IANA>          — Set your timezone (e.g. Europe/Paris)",
            "  /lang en|fr               — Set bot language",
            "  /setprofile <desc>        — Set your AI profile",
            "  /profile                  — Show your AI profile",
            "  /dailyrecap [on|off|at HH:MM]",
            "  /nudges [on|off|at N]     — Manage inactivity reminders",
        ]);

        if is_admin {
            full_lines.extend(str_lines(&[
                "",
                "Admin",
                "  /useradd <id|@user>       — Add a user",
                "  /userremove <id|@user>    — Remove a user",
                "  /userlist                 — List all users",
                "  /usersetrole <id|@user> <role>",
                "  /usercost <id|@user> [7d|30d|all] — Show estimated AI spend for a user",
                "  /topusage [limit]         — Top users by estimated AI spend",
                "  /enablewebsearch <id|@user>",
                "  /disablewebsearch <id|@user>",
                "  /setsportemoji <emoji>    — Set sport reaction emoji for this chat",
                "  /monthlygraph [on|off|now] — Schedule monthly training graph (admin)",
            ]));
        }

        full_lines.extend(str_lines(&[
            "",
            "Proposals",
            "  /propose <text>           — Propose a new feature",
            "  /proposals                — List feature proposals",
            "  /delproposal <id>         — Delete a proposal",
        ]));

        full_lines.join("\n")
    }

    fn help_fr(&self, role: &Role, full: bool) -> String {
        let mut lines: Vec<String> = Vec::new();

        let is_admin = matches!(role, Role::Admin);

        lines.push("📋 Aide rapide — tapez /help all pour la liste complète :".to_string());
        lines.push("".to_string());
        lines.push("Corps & Fitness".to_string());
        lines.push("  /weight <kg>        — Enregistrer votre poids".to_string());
        lines.push("  /lastweight         — Dernier poids enregistré".to_string());
        lines.push("  /weightchart [30|90|all] — Courbe de poids".to_string());
        lines.push("  /nutrition [date]   — Tableau nutrition du jour".to_string());
        lines.push("  /training <desc>    — Enregistrer une séance".to_string());
        lines.push("  /lasttraining [n]   — Vos dernières séances (défaut 5)".to_string());
        lines.push("  /showtraining <id>  — Fiche d'une séance".to_string());
        lines.push("  /updatetraining <id> <desc> — Modifier une séance".to_string());
        lines.push("  /deltraining <id>   — Supprimer une séance".to_string());
        lines.push("  /streak             — Vos séries en cours".to_string());
        lines.push("  /monthlygraph last|4w|3m  — Graphique du groupe".to_string());

        lines.push("".to_string());
        lines.push("IA".to_string());
        lines.push("  /ask <question>     — Demander à l'IA".to_string());

        lines.push("".to_string());
        lines.push("Tâches".to_string());
        lines.push("  /task <titre>       — Créer une tâche".to_string());
        lines.push("  /tasks              — Lister vos tâches".to_string());
        lines.push("  /done <id>          — Marquer une tâche terminée".to_string());

        lines.push("".to_string());
        lines.push("Rappels".to_string());
        lines.push("  /remind <quand> ... — Définir un rappel".to_string());

        lines.push("".to_string());
        lines.push("Paramètres".to_string());
        lines.push("  /timezone <IANA>    — Définir votre fuseau horaire".to_string());
        lines.push("  /lang en|fr         — Langue du bot".to_string());
        lines.push("  /setprofile <desc>  — Définir votre profil IA".to_string());
        lines.push("  /dailyrecap [on|off|at HH:MM]".to_string());
        lines.push("  /nudges [on|off|at N]".to_string());

        if is_admin {
            lines.push("".to_string());
            lines.push("Admin".to_string());
            lines.push("  /useradd <id|@user> — Ajouter un utilisateur".to_string());
            lines.push("  /userlist           — Lister les utilisateurs".to_string());
            lines.push("  /topusage [limite]  — Top dépense IA".to_string());
            lines.push("  /setsportemoji <emoji>".to_string());
            lines.push("  /monthlygraph [on|off|now]".to_string());
        }

        lines.push("".to_string());
        lines.push("Tapez /help all pour voir toutes les commandes par catégorie.".to_string());

        if !full {
            return lines.join("\n");
        }

        let mut full_lines = str_lines(&[
            "📖 Liste complète des commandes :",
            "",
            "Corps & Fitness",
            "  /weight <kg>              — Enregistrer votre poids en kg",
            "  /lastweight               — Votre dernière entrée de poids",
            "  /weightchart [30|90|all]  — Courbe de poids",
            "  /nutrition [today|YYYY-MM-DD] — Tableau nutrition du jour (privé)",
            "  /training <desc>          — Enregistrer une séance d'entraînement",
            "  /lasttraining [n]         — Vos n dernières séances (défaut 5)",
            "  /showtraining <id>         — Fiche complète d'une séance (admin : toute)",
            "  /updatetraining <id> <desc> — Modifier votre séance (admin : toute)",
            "  /deltraining <id>         — Supprimer votre séance (admin : toute)",
            "  /streak                   — Vos séries de poids et d'entraînement",
            "  /monthlygraph last|4w|3m  — Graphique du groupe (mois calendaire, semaines ou mois)",
            "",
            "Objectifs",
            "  /setgoal <type> <valeur>  — Définir un objectif (weight 75.0 ou training 3)",
            "  /goals                    — Objectifs actifs et progression",
            "  /delgoal <id>             — Supprimer un objectif",
            "",
            "Tâches",
            "  /task <titre>             — Créer une tâche",
            "  /tasks                    — Lister vos tâches",
            "  /done <id>                — Marquer une tâche comme terminée",
            "  /canceltask <id>          — Annuler une tâche",
            "  /deltask <id>             — Supprimer une tâche définitivement",
            "",
            "Rappels",
            "  /remind <quand> <msg>     — Rappel unique ou récurrent",
            "  /reminders                — Rappels en attente",
            "  /cancelremind <id>        — Annuler un rappel",
            "",
            "IA & Paramètres",
            "  /ask <question>           — Demander à l'IA",
            "  /timezone <IANA>          — Définir votre fuseau (ex. Europe/Paris)",
            "  /lang en|fr               — Langue du bot",
            "  /setprofile <desc>        — Définir votre profil IA",
            "  /profile                  — Afficher votre profil IA",
            "  /dailyrecap [on|off|at HH:MM]",
            "  /nudges [on|off|at N]     — Gérer les rappels d'inactivité",
        ]);

        if is_admin {
            full_lines.extend(str_lines(&[
                "",
                "Admin",
                "  /useradd <id|@user>       — Ajouter un utilisateur",
                "  /userremove <id|@user>    — Supprimer un utilisateur",
                "  /userlist                 — Lister les utilisateurs",
                "  /usersetrole <id|@user> <rôle>",
                "  /usercost <id|@user> [7d|30d|all] — Dépense IA estimée d'un utilisateur",
                "  /topusage [limite]        — Top utilisateurs par dépense IA estimée",
                "  /enablewebsearch <id|@user>",
                "  /disablewebsearch <id|@user>",
                "  /setsportemoji <emoji>    — Emoji de réaction sport pour ce chat",
                "  /monthlygraph [on|off|now] — Planifier le graphique mensuel (admin)",
            ]));
        }

        full_lines.extend(str_lines(&[
            "",
            "Propositions",
            "  /propose <texte>          — Proposer une fonctionnalité",
            "  /proposals                — Lister les propositions",
            "  /delproposal <id>         — Supprimer une proposition",
        ]));

        full_lines.join("\n")
    }

    // ── Weight ──────────────────────────────────────────────────────────────
    pub fn weight_invalid(&self) -> &'static str {
        match self.lang {
            Lang::En => "Please provide a valid weight, e.g. /weight 82.5",
            Lang::Fr => "Veuillez fournir un poids valide, ex. /weight 82.5",
        }
    }

    pub fn weight_recorded(&self, kg: f64, date: &str) -> String {
        match self.lang {
            Lang::En => format!("Recorded weight: {:.1} kg on {}", kg, date),
            Lang::Fr => format!("Poids enregistré : {:.1} kg le {}", kg, date),
        }
    }

    pub fn weight_save_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to save weight entry. Please try again.",
            Lang::Fr => "Échec de l'enregistrement du poids. Veuillez réessayer.",
        }
    }

    pub fn weight_fetch_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to fetch weight data.",
            Lang::Fr => "Échec de la récupération des données de poids.",
        }
    }

    pub fn weight_no_entries(&self) -> &'static str {
        match self.lang {
            Lang::En => "No weight entries yet. Use /weight 82.5 to record one.",
            Lang::Fr => {
                "Aucune entrée de poids pour le moment. Utilisez /weight 82.5 pour enregistrer."
            }
        }
    }

    pub fn weight_last(&self, kg: f64, date: &str) -> String {
        match self.lang {
            Lang::En => format!("Last weight: {:.1} kg ({})", kg, date),
            Lang::Fr => format!("Dernier poids : {:.1} kg ({})", kg, date),
        }
    }

    pub fn weight_chart_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /weightchart [30|90|all] — default is 90 days.",
            Lang::Fr => "Usage : /weightchart [30|90|all] — 90 jours par défaut.",
        }
    }

    pub fn weight_chart_no_data_in_window(&self) -> &'static str {
        match self.lang {
            Lang::En => "No weight entries in that time window. Try /weightchart all.",
            Lang::Fr => "Aucune entrée de poids sur cette période. Essayez /weightchart all.",
        }
    }

    pub fn weight_chart_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to generate the weight chart.",
            Lang::Fr => "Échec de la génération de la courbe de poids.",
        }
    }

    pub fn weight_chart_caption(
        &self,
        points: usize,
        start_kg: f64,
        end_kg: f64,
        delta_kg: f64,
        goal_kg: Option<f64>,
        window: &str,
    ) -> String {
        let delta_sign = if delta_kg > 0.0 { "+" } else { "" };
        match self.lang {
            Lang::En => {
                let mut s = format!(
                    "Weight chart ({window}) — {points} day(s)\n\
                     {start_kg:.1} → {end_kg:.1} kg ({delta_sign}{delta_kg:.1} kg)"
                );
                if let Some(goal) = goal_kg {
                    s.push_str(&format!("\nGoal: {goal:.1} kg"));
                }
                s
            }
            Lang::Fr => {
                let mut s = format!(
                    "Courbe de poids ({window}) — {points} jour(s)\n\
                     {start_kg:.1} → {end_kg:.1} kg ({delta_sign}{delta_kg:.1} kg)"
                );
                if let Some(goal) = goal_kg {
                    s.push_str(&format!("\nObjectif : {goal:.1} kg"));
                }
                s
            }
        }
    }

    // ── Nutrition ────────────────────────────────────────────────────────────
    pub fn nutrition_title(&self) -> &'static str {
        match self.lang {
            Lang::En => "Nutrition",
            Lang::Fr => "Nutrition",
        }
    }

    pub fn nutrition_total(&self) -> &'static str {
        match self.lang {
            Lang::En => "TOTAL",
            Lang::Fr => "TOTAL",
        }
    }

    pub fn nutrition_carbs_short(&self) -> &'static str {
        match self.lang {
            Lang::En => "C",
            Lang::Fr => "G",
        }
    }

    pub fn nutrition_fat_short(&self) -> &'static str {
        match self.lang {
            Lang::En => "F",
            Lang::Fr => "L",
        }
    }

    pub fn nutrition_kind(&self, kind: crate::db::meals::MealKind) -> &'static str {
        use crate::db::meals::MealKind::*;
        match (self.lang, kind) {
            (Lang::En, Breakfast) => "Breakfast",
            (Lang::En, Lunch) => "Lunch",
            (Lang::En, Dinner) => "Dinner",
            (Lang::En, Snack) => "Snack",
            (Lang::En, Other) => "Meal",
            (Lang::Fr, Breakfast) => "Petit-déj",
            (Lang::Fr, Lunch) => "Déjeuner",
            (Lang::Fr, Dinner) => "Dîner",
            (Lang::Fr, Snack) => "Collation",
            (Lang::Fr, Other) => "Repas",
        }
    }

    pub fn nutrition_caption(
        &self,
        date: &str,
        kcal: f64,
        protein_g: f64,
        carbs_g: f64,
        fat_g: f64,
    ) -> String {
        match self.lang {
            Lang::En => format!(
                "Nutrition {date} — {kcal:.0} kcal · P {protein_g:.0}g · C {carbs_g:.0}g · F {fat_g:.0}g"
            ),
            Lang::Fr => format!(
                "Nutrition {date} — {kcal:.0} kcal · P {protein_g:.0}g · G {carbs_g:.0}g · L {fat_g:.0}g"
            ),
        }
    }

    pub fn nutrition_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /nutrition [today|yesterday|YYYY-MM-DD]",
            Lang::Fr => "Usage : /nutrition [today|yesterday|YYYY-MM-DD]",
        }
    }

    pub fn nutrition_empty(&self, date: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "No meals logged for {date}. Tell me what you ate, or register a food first."
            ),
            Lang::Fr => format!(
                "Aucun repas enregistré pour {date}. Dis-moi ce que tu as mangé, ou enregistre un aliment."
            ),
        }
    }

    pub fn nutrition_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to generate the nutrition table.",
            Lang::Fr => "Échec de la génération du tableau nutrition.",
        }
    }

    pub fn nutrition_private_only(&self) -> &'static str {
        match self.lang {
            Lang::En => "Nutrition is private. Open a 1:1 chat with me for /nutrition.",
            Lang::Fr => "La nutrition est privée. Ouvre une conversation 1:1 pour /nutrition.",
        }
    }

    pub fn nutrition_fetch_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to load nutrition data.",
            Lang::Fr => "Impossible de charger les données nutrition.",
        }
    }

    // ── Training ─────────────────────────────────────────────────────────────
    pub fn training_empty(&self) -> &'static str {
        match self.lang {
            Lang::En => "Please describe the session, e.g. /training Ran 5km",
            Lang::Fr => "Veuillez décrire la séance, ex. /training Course 5km",
        }
    }

    pub fn training_logged(&self, description: &str, date: &str) -> String {
        match self.lang {
            Lang::En => format!("Logged training: \"{}\" on {}", description, date),
            Lang::Fr => format!("Entraînement enregistré : \"{}\" le {}", description, date),
        }
    }

    pub fn training_already_logged(&self, id: i64, date: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "Already logged (#{id} on {date}). Not added again — use /updatetraining or /showtraining {id}."
            ),
            Lang::Fr => format!(
                "Déjà enregistré (#{id} le {date}). Pas de doublon — /updatetraining ou /showtraining {id}."
            ),
        }
    }

    pub fn training_save_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to save training entry. Please try again.",
            Lang::Fr => "Échec de l'enregistrement de l'entraînement. Veuillez réessayer.",
        }
    }

    pub fn training_fetch_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to fetch training data.",
            Lang::Fr => "Échec de la récupération des données d'entraînement.",
        }
    }

    pub fn training_no_entries(&self) -> &'static str {
        match self.lang {
            Lang::En => "No training entries yet. Use /training <description> to log one.",
            Lang::Fr => {
                "Aucune entrée d'entraînement pour le moment. Utilisez /training <description> pour enregistrer."
            }
        }
    }

    pub fn training_sessions_header(&self) -> &'static str {
        match self.lang {
            Lang::En => "Last training sessions:\n",
            Lang::Fr => "Dernières séances d'entraînement :\n",
        }
    }

    pub fn training_session_entry(&self, id: i64, date: &str, description: &str) -> String {
        match self.lang {
            Lang::En => format!("• #{id} {date} — {description}"),
            Lang::Fr => format!("• #{id} {date} — {description}"),
        }
    }

    pub fn training_update_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /updatetraining <id> <new description>",
            Lang::Fr => "Usage : /updatetraining <id> <nouvelle description>",
        }
    }

    pub fn training_delete_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /deltraining <id>",
            Lang::Fr => "Usage : /deltraining <id>",
        }
    }

    pub fn training_show_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /showtraining <id>",
            Lang::Fr => "Usage : /showtraining <id>",
        }
    }

    pub fn training_updated(&self, id: i64, description: &str, date: &str) -> String {
        match self.lang {
            Lang::En => format!("Updated training #{id}: \"{description}\" ({date})"),
            Lang::Fr => format!("Séance #{id} mise à jour : \"{description}\" ({date})"),
        }
    }

    pub fn training_deleted(&self, id: i64) -> String {
        match self.lang {
            Lang::En => format!("Training #{id} deleted."),
            Lang::Fr => format!("Séance #{id} supprimée."),
        }
    }

    pub fn training_not_found_or_forbidden(&self) -> &'static str {
        match self.lang {
            Lang::En => "Training not found, or you are not allowed to change it.",
            Lang::Fr => "Séance introuvable, ou vous n'êtes pas autorisé à la modifier.",
        }
    }

    pub fn training_update_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update training entry.",
            Lang::Fr => "Échec de la mise à jour de la séance.",
        }
    }

    pub fn training_delete_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to delete training entry.",
            Lang::Fr => "Échec de la suppression de la séance.",
        }
    }

    pub fn training_photo_reaction(&self, reactor_display: &str, emoji: &str) -> String {
        match self.lang {
            Lang::En => format!("Photo ({} by {})", emoji, reactor_display),
            Lang::Fr => format!("Photo ({} par {})", emoji, reactor_display),
        }
    }

    pub fn ai_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Sorry, I couldn't process your question. Please try again.",
            Lang::Fr => "Désolé, je n'ai pas pu traiter votre question. Veuillez réessayer.",
        }
    }

    pub fn ai_empty_response(&self) -> &'static str {
        match self.lang {
            Lang::En => "I couldn't generate a response. Please try rephrasing.",
            Lang::Fr => "Je n'ai pas pu générer de réponse. Essayez de reformuler.",
        }
    }

    pub fn ai_tool_loop_limit(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "I looked through your data but had to stop repeating the same kind of search. Ask again with a more specific question if you need more."
            }
            Lang::Fr => {
                "J'ai cherché dans tes données mais j'ai dû arrêter de relancer le même type de recherche. Relance avec une question plus précise si tu as besoin de plus."
            }
        }
    }

    /// Ceiling reply when this turn already deleted or logged something.
    pub fn ai_tool_loop_limit_after_writes(&self, writes: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "I already changed your data this turn: {writes}. I stopped before finishing the rest. Ask again if something is still wrong."
            ),
            Lang::Fr => format!(
                "J'ai déjà modifié tes données pendant ce tour : {writes}. Je me suis arrêté avant de finir le reste. Relance si quelque chose ne va pas."
            ),
        }
    }

    pub fn ai_no_api_key(&self) -> &'static str {
        match self.lang {
            Lang::En => "AI features are not configured. Contact the admin.",
            Lang::Fr => {
                "Les fonctionnalités IA ne sont pas configurées. Contactez l'administrateur."
            }
        }
    }

    pub fn ask_description(&self) -> &'static str {
        match self.lang {
            Lang::En => "Ask the AI anything: /ask How am I doing today?",
            Lang::Fr => "Demandez ce que vous voulez à l'IA : /ask Comment je vais aujourd'hui ?",
        }
    }

    pub fn web_search_set(&self, telegram_user_id: i64, status: &str) -> String {
        match self.lang {
            Lang::En => format!("Web search {status} for user {telegram_user_id}."),
            Lang::Fr => format!("Recherche web {status} pour l'utilisateur {telegram_user_id}.",),
        }
    }

    pub fn web_search_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update web search setting.",
            Lang::Fr => "Échec de la mise à jour du paramètre de recherche web.",
        }
    }

    // ── Task ──────────────────────────────────────────────────────────────────

    pub fn task_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /task <title> — Create a new task.",
            Lang::Fr => "Utilisation : /task <titre> — Créer une nouvelle tâche.",
        }
    }

    pub fn task_created(&self, id: i64, title: &str) -> String {
        match self.lang {
            Lang::En => format!("#{} Task created: {}", id, title),
            Lang::Fr => format!("#{} Tâche créée : {}", id, title),
        }
    }

    pub fn task_create_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to create task.",
            Lang::Fr => "Échec de la création de la tâche.",
        }
    }

    pub fn task_no_tasks(&self) -> &'static str {
        match self.lang {
            Lang::En => "No tasks yet. Use /task <title> to create one.",
            Lang::Fr => "Aucune tâche pour le moment. Utilisez /task <titre> pour en créer une.",
        }
    }

    pub fn task_list_header(&self) -> String {
        match self.lang {
            Lang::En => "Your tasks:\n".to_string(),
            Lang::Fr => "Vos tâches :\n".to_string(),
        }
    }

    pub fn task_list_entry(&self, id: i64, title: &str, status: &str, priority: &str) -> String {
        match self.lang {
            Lang::En => format!("#{} [{}] {} ({})\n", id, status, priority, title),
            Lang::Fr => format!("#{} [{}] {} ({})\n", id, status, priority, title),
        }
    }

    pub fn task_list_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to list tasks.",
            Lang::Fr => "Échec de la liste des tâches.",
        }
    }

    pub fn task_done_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /done <task_id>",
            Lang::Fr => "Utilisation : /done <id_tâche>",
        }
    }

    pub fn task_marked_done(&self, title: &str) -> String {
        match self.lang {
            Lang::En => format!("Task marked as done: {}", title),
            Lang::Fr => format!("Tâche marquée comme terminée : {}", title),
        }
    }

    pub fn task_update_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update task.",
            Lang::Fr => "Échec de la mise à jour de la tâche.",
        }
    }

    pub fn task_cancel_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /canceltask <task_id>",
            Lang::Fr => "Utilisation : /canceltask <id_tâche>",
        }
    }

    pub fn task_cancelled(&self, title: &str) -> String {
        match self.lang {
            Lang::En => format!("Task cancelled: {}", title),
            Lang::Fr => format!("Tâche annulée : {}", title),
        }
    }

    pub fn task_delete_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /deltask <task_id> — Permanently delete a task.",
            Lang::Fr => "Utilisation : /deltask <id_tâche> — Supprimer définitivement une tâche.",
        }
    }

    pub fn task_deleted(&self, id: i64) -> String {
        match self.lang {
            Lang::En => format!("Task #{id} deleted."),
            Lang::Fr => format!("Tâche #{id} supprimée."),
        }
    }

    pub fn task_not_found(&self) -> &'static str {
        match self.lang {
            Lang::En => "Task not found or not yours.",
            Lang::Fr => "Tâche introuvable ou ne vous appartient pas.",
        }
    }

    pub fn task_delete_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to delete task.",
            Lang::Fr => "Échec de la suppression de la tâche.",
        }
    }

    // ── Reminders ─────────────────────────────────────────────────────────────

    pub fn remind_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "Usage: /remind <message> — Set a one-time reminder in 1 hour.\n\
                 /remind daily 9am <message> — Daily reminder\n\
                 /remind weekly monday 9am <message> — Weekly reminder\n\
                 /remind monthly 1 9am <message> — Monthly reminder\n\
                 /remind every day at 9am <message> — Natural form"
            }
            Lang::Fr => {
                "Utilisation : /remind <message> — Rappel unique dans 1 heure.\n\
                 /remind daily 9am <message> — Rappel quotidien\n\
                 /remind weekly monday 9am <message> — Rappel hebdomadaire\n\
                 /remind monthly 1 9am <message> — Rappel mensuel\n\
                 /remind every day at 9am <message> — Forme naturelle"
            }
        }
    }

    pub fn remind_created(&self, when: &str, message: &str, recurrence: &str) -> String {
        let recurrence_label = match self.lang {
            Lang::En => match recurrence {
                "daily" => "daily",
                "weekly" => "weekly",
                "monthly" => "monthly",
                _ => "one-time",
            },
            Lang::Fr => match recurrence {
                "daily" => "quotidien",
                "weekly" => "hebdomadaire",
                "monthly" => "mensuel",
                _ => "unique",
            },
        };
        match self.lang {
            Lang::En => format!(
                "Reminder set for {} ({}): {}",
                when, recurrence_label, message
            ),
            Lang::Fr => format!(
                "Rappel défini pour {} ({}): {}",
                when, recurrence_label, message
            ),
        }
    }

    pub fn remind_create_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to create reminder.",
            Lang::Fr => "Échec de la création du rappel.",
        }
    }

    pub fn remind_no_reminders(&self) -> &'static str {
        match self.lang {
            Lang::En => "No pending reminders. Use /remind <when> <message> to set one.",
            Lang::Fr => {
                "Aucun rappel en attente. Utilisez /remind <quand> <message> pour en définir un."
            }
        }
    }

    pub fn remind_list_header(&self) -> String {
        match self.lang {
            Lang::En => "Pending reminders:\n".to_string(),
            Lang::Fr => "Rappels en attente :\n".to_string(),
        }
    }

    pub fn remind_list_entry(&self, id: i64, scheduled: &str, recurrence: &str) -> String {
        let recurrence_label = match self.lang {
            Lang::En => match recurrence {
                "daily" => "daily",
                "weekly" => "weekly",
                "monthly" => "monthly",
                _ => "one-time",
            },
            Lang::Fr => match recurrence {
                "daily" => "quotidien",
                "weekly" => "hebdomadaire",
                "monthly" => "mensuel",
                _ => "unique",
            },
        };
        match self.lang {
            Lang::En => format!("#{} - {} ({})\n", id, scheduled, recurrence_label),
            Lang::Fr => format!("#{} - {} ({})\n", id, scheduled, recurrence_label),
        }
    }

    pub fn remind_list_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to list reminders.",
            Lang::Fr => "Échec de la liste des rappels.",
        }
    }

    pub fn remind_cancel_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to cancel reminder.",
            Lang::Fr => "Échec de l'annulation du rappel.",
        }
    }

    pub fn remind_cancel_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /cancelremind <reminder_id> — Cancel a reminder by ID.",
            Lang::Fr => "Utilisation : /cancelremind <id> — Annuler un rappel par ID.",
        }
    }

    pub fn remind_cancelled(&self, id: i64) -> String {
        match self.lang {
            Lang::En => format!("Reminder #{id} cancelled."),
            Lang::Fr => format!("Rappel #{id} annulé."),
        }
    }

    pub fn remind_not_found(&self) -> &'static str {
        match self.lang {
            Lang::En => "Reminder not found or not yours.",
            Lang::Fr => "Rappel introuvable ou ne vous appartient pas.",
        }
    }

    pub fn remind_target_custom(&self, msg: &str) -> String {
        match self.lang {
            Lang::En => format!("Reminder: {}", msg),
            Lang::Fr => format!("Rappel : {}", msg),
        }
    }

    pub fn remind_target_task(&self, task_title: &str) -> String {
        match self.lang {
            Lang::En => format!("Task reminder: {}", task_title),
            Lang::Fr => format!("Rappel de tâche : {}", task_title),
        }
    }

    // ── Profile ────────────────────────────────────────────────────────────────

    pub fn profile_set_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /setprofile <description> — Set your AI profile.",
            Lang::Fr => "Utilisation : /setprofile <description> — Définir votre profil IA.",
        }
    }

    pub fn profile_updated(&self) -> &'static str {
        match self.lang {
            Lang::En => "Profile updated!",
            Lang::Fr => "Profil mis à jour !",
        }
    }

    pub fn profile_view(&self, desc: &str) -> String {
        match self.lang {
            Lang::En => format!("Your profile:\n{desc}"),
            Lang::Fr => format!("Votre profil :\n{desc}"),
        }
    }

    pub fn profile_empty(&self) -> &'static str {
        match self.lang {
            Lang::En => "No profile set. Use /setprofile <description> to tell me about yourself.",
            Lang::Fr => {
                "Aucun profil défini. Utilisez /setprofile <description> pour me parler de vous."
            }
        }
    }

    pub fn profile_update_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update profile.",
            Lang::Fr => "Échec de la mise à jour du profil.",
        }
    }

    // ── Daily Recap ───────────────────────────────────────────────────────

    pub fn daily_recap_status(&self, enabled: bool, hour: i64, minute: i64) -> String {
        match self.lang {
            Lang::En => {
                if enabled {
                    format!("Daily recap: ON at {hour:02}:{minute:02} UTC")
                } else {
                    "Daily recap: OFF".to_string()
                }
            }
            Lang::Fr => {
                if enabled {
                    format!("Récap journalier : ACTIVÉ à {hour:02}:{minute:02} UTC")
                } else {
                    "Récap journalier : DÉSACTIVÉ".to_string()
                }
            }
        }
    }

    pub fn daily_recap_enabled(&self, hour: i64, minute: i64) -> String {
        match self.lang {
            Lang::En => format!("Daily recap enabled at {hour:02}:{minute:02} UTC."),
            Lang::Fr => format!("Récap journalier activé à {hour:02}:{minute:02} UTC."),
        }
    }

    pub fn daily_recap_disabled(&self) -> &'static str {
        match self.lang {
            Lang::En => "Daily recap disabled.",
            Lang::Fr => "Récap journalier désactivé.",
        }
    }

    pub fn daily_recap_time_set(&self, hour: i64, minute: i64) -> String {
        match self.lang {
            Lang::En => format!("Daily recap time set to {hour:02}:{minute:02} UTC."),
            Lang::Fr => format!("Heure du récap journalier définie à {hour:02}:{minute:02} UTC."),
        }
    }

    pub fn daily_recap_invalid_time(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "Invalid time format. Use: /dailyrecap at HH:MM (e.g. /dailyrecap at 20:00)"
            }
            Lang::Fr => {
                "Format d'heure invalide. Utilisez : /dailyrecap at HH:MM (ex: /dailyrecap at 20:00)"
            }
        }
    }

    pub fn daily_recap_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update daily recap settings.",
            Lang::Fr => "Échec de la mise à jour des paramètres du récap journalier.",
        }
    }

    // ── Feature Proposals ──────────────────────────────────────────────────
    pub fn proposal_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /propose <text> — Propose a feature.",
            Lang::Fr => "Usage : /propose <texte> — Proposer une fonctionnalité.",
        }
    }

    pub fn proposal_created(&self, id: i64, title: &str) -> String {
        match self.lang {
            Lang::En => format!("#{} Proposal created: {}", id, title),
            Lang::Fr => format!("#{} Proposition créée : {}", id, title),
        }
    }

    pub fn proposal_create_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to create proposal.",
            Lang::Fr => "Échec de la création de la proposition.",
        }
    }

    pub fn proposal_list_header_mine(&self) -> String {
        match self.lang {
            Lang::En => "Your proposals:\n".to_string(),
            Lang::Fr => "Vos propositions :\n".to_string(),
        }
    }

    pub fn proposal_list_header_all(&self) -> String {
        match self.lang {
            Lang::En => "All proposals:\n".to_string(),
            Lang::Fr => "Toutes les propositions :\n".to_string(),
        }
    }

    pub fn proposal_list_entry(&self, id: i64, title: &str, author: &str) -> String {
        match self.lang {
            Lang::En => format!("#{} {}{}\n", id, title, author),
            Lang::Fr => format!("#{} {}{}\n", id, title, author),
        }
    }

    pub fn proposal_list_empty(&self) -> &'static str {
        match self.lang {
            Lang::En => "No proposals yet. Use /propose <text> to suggest a feature.",
            Lang::Fr => {
                "Aucune proposition pour le moment. Utilisez /propose <texte> pour suggérer une fonctionnalité."
            }
        }
    }

    pub fn proposal_list_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to list proposals.",
            Lang::Fr => "Échec de la liste des propositions.",
        }
    }

    pub fn proposal_not_found(&self) -> &'static str {
        match self.lang {
            Lang::En => "Proposal not found or not yours.",
            Lang::Fr => "Proposition introuvable ou ne vous appartenant pas.",
        }
    }

    pub fn proposal_delete_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /delproposal <id> — Delete a proposal.",
            Lang::Fr => "Usage : /delproposal <id> — Supprimer une proposition.",
        }
    }

    pub fn proposal_deleted(&self, id: i64) -> String {
        match self.lang {
            Lang::En => format!("Proposal #{} deleted.", id),
            Lang::Fr => format!("Proposition #{} supprimée.", id),
        }
    }

    pub fn proposal_delete_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to delete proposal.",
            Lang::Fr => "Échec de la suppression de la proposition.",
        }
    }

    // ── AI Usage / Cost ───────────────────────────────────────────────────

    pub fn user_cost_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /usercost <chat_id|@username> [7d|30d|all]",
            Lang::Fr => "Usage : /usercost <chat_id|@username> [7d|30d|all]",
        }
    }

    pub fn user_cost(&self, r: UserCostReport<'_>) -> String {
        let telegram_user_id = r.telegram_user_id;
        let window = r.window;
        let request_count = r.request_count;
        let prompt_tokens = r.prompt_tokens;
        let completion_tokens = r.completion_tokens;
        let total_tokens = r.total_tokens;
        let cached_tokens = r.cached_tokens;
        let cache_usd_saved = r.cache_usd_saved;
        let estimated_cost = r.estimated_cost;
        let mut breakdown = String::new();
        if !r.model_lines.is_empty() {
            breakdown.push('\n');
            breakdown.push_str(match self.lang {
                Lang::En => "By model:",
                Lang::Fr => "Par modèle :",
            });
            breakdown.push('\n');
            breakdown.push_str(&r.model_lines.join("\n"));
        }
        if !r.kind_lines.is_empty() {
            breakdown.push('\n');
            breakdown.push_str(match self.lang {
                Lang::En => "By purpose:",
                Lang::Fr => "Par usage :",
            });
            breakdown.push('\n');
            breakdown.push_str(&r.kind_lines.join("\n"));
        }
        match self.lang {
            Lang::En => format!(
                "AI spend for user {telegram_user_id} ({window}):\n\
                 Estimated cost: ${estimated_cost:.4} (paid-tier Standard, approx.)\n\
                 Requests: {request_count}\n\
                 Tokens: {prompt_tokens} in / {completion_tokens} out ({total_tokens} total)\n\
                 Cache: {cached_tokens} tok, saved ~${cache_usd_saved:.4}{breakdown}"
            ),
            Lang::Fr => format!(
                "Dépense IA pour l'utilisateur {telegram_user_id} ({window}) :\n\
                 Coût estimé : {estimated_cost:.4} $ (tarif Standard, approx.)\n\
                 Requêtes : {request_count}\n\
                 Tokens : {prompt_tokens} entrée / {completion_tokens} sortie ({total_tokens} total)\n\
                 Cache : {cached_tokens} tok, économie ~{cache_usd_saved:.4} ${breakdown}"
            ),
        }
    }

    pub fn user_cost_model_line(
        &self,
        model: &str,
        request_count: i64,
        prompt_tokens: i64,
        completion_tokens: i64,
        cached_tokens: i64,
        estimated_cost: f64,
    ) -> String {
        match self.lang {
            Lang::En => format!(
                "  {model}: {request_count} req, {prompt_tokens} in / {completion_tokens} out, {cached_tokens} cache (~${estimated_cost:.4})"
            ),
            Lang::Fr => format!(
                "  {model} : {request_count} req, {prompt_tokens} entrée / {completion_tokens} sortie, {cached_tokens} cache (~${estimated_cost:.4})"
            ),
        }
    }

    pub fn user_cost_kind_line(
        &self,
        kind: &str,
        request_count: i64,
        estimated_cost: f64,
    ) -> String {
        match self.lang {
            Lang::En => format!("  {kind}: {request_count} req (~${estimated_cost:.4})"),
            Lang::Fr => format!("  {kind} : {request_count} req (~${estimated_cost:.4})"),
        }
    }

    pub fn user_cost_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to fetch AI usage for this user.",
            Lang::Fr => "Échec de la récupération de l'utilisation IA pour cet utilisateur.",
        }
    }

    pub fn top_usage_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /topusage [limit] (1–25, default 10)",
            Lang::Fr => "Usage : /topusage [limite] (1–25, défaut 10)",
        }
    }

    pub fn top_usage_empty(&self) -> &'static str {
        match self.lang {
            Lang::En => "No AI usage recorded yet.",
            Lang::Fr => "Aucune utilisation IA enregistrée pour le moment.",
        }
    }

    pub fn top_usage_header(&self, count: usize) -> String {
        match self.lang {
            Lang::En => format!("Top {count} users by estimated AI spend:"),
            Lang::Fr => format!("Top {count} utilisateurs par dépense IA estimée :"),
        }
    }

    pub fn top_usage_row(
        &self,
        rank: usize,
        label: &str,
        request_count: i64,
        total_tokens: i64,
        estimated_cost: f64,
    ) -> String {
        match self.lang {
            Lang::En => format!(
                "{rank}. {label} — ~${estimated_cost:.4} ({request_count} req, {total_tokens} tok)"
            ),
            Lang::Fr => format!(
                "{rank}. {label} — ~${estimated_cost:.4} ({request_count} req, {total_tokens} tok)"
            ),
        }
    }

    pub fn monthly_graph_group_only(&self) -> &'static str {
        match self.lang {
            Lang::En => "Monthly training graphs are only available in group chats.",
            Lang::Fr => {
                "Les graphiques d'entraînement mensuels ne sont disponibles que dans les groupes."
            }
        }
    }

    pub fn monthly_graph_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "Usage: /monthlygraph last — previous calendar month\n\
                 /monthlygraph 4w — last 4 weeks (1w–52w)\n\
                 /monthlygraph 3m — last 3 months (1m–24m)\n\
                 Admin: /monthlygraph [on|off|now|status]"
            }
            Lang::Fr => {
                "Usage : /monthlygraph last — mois calendaire précédent\n\
                 /monthlygraph 4w — 4 dernières semaines (1w–52w)\n\
                 /monthlygraph 3m — 3 derniers mois (1m–24m)\n\
                 Admin : /monthlygraph [on|off|now|status]"
            }
        }
    }

    pub fn monthly_graph_status(&self, enabled: bool) -> String {
        match self.lang {
            Lang::En => format!(
                "Monthly training graph for this group: {}",
                if enabled { "ON" } else { "OFF" }
            ),
            Lang::Fr => format!(
                "Graphique d'entraînement mensuel pour ce groupe : {}",
                if enabled { "ACTIVÉ" } else { "DÉSACTIVÉ" }
            ),
        }
    }

    pub fn monthly_graph_enabled(&self) -> &'static str {
        match self.lang {
            Lang::En => {
                "Monthly training graph enabled. It will be sent on the last day of each month."
            }
            Lang::Fr => {
                "Graphique d'entraînement mensuel activé. Il sera envoyé le dernier jour de chaque mois."
            }
        }
    }

    pub fn monthly_graph_disabled(&self) -> &'static str {
        match self.lang {
            Lang::En => "Monthly training graph disabled for this group.",
            Lang::Fr => "Graphique d'entraînement mensuel désactivé pour ce groupe.",
        }
    }

    pub fn monthly_graph_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to generate or send the monthly training graph.",
            Lang::Fr => "Échec de la génération ou de l'envoi du graphique d'entraînement mensuel.",
        }
    }

    // ── Streaks & Nudges ──────────────────────────────────────────────────

    pub fn streak_message(&self, weight_streak: i64, training_streak: i64) -> String {
        match self.lang {
            Lang::En => format!(
                "Your current streaks:\n\
                 Weight: {} day{}\n\
                 Training: {} day{}\n\
                 Keep it going!",
                weight_streak,
                if weight_streak == 1 { "" } else { "s" },
                training_streak,
                if training_streak == 1 { "" } else { "s" }
            ),
            Lang::Fr => format!(
                "Vos séries actuelles :\n\
                 Poids : {} jour{}\n\
                 Entraînement : {} jour{}\n\
                 Continuez comme ça !",
                weight_streak,
                if weight_streak == 1 { "" } else { "s" },
                training_streak,
                if training_streak == 1 { "" } else { "s" }
            ),
        }
    }

    pub fn streak_empty(&self) -> String {
        match self.lang {
            Lang::En => "No active streaks yet.\n\
                 Start one by logging your weight with /weight 82.5 or a workout with /training.".to_string(),
            Lang::Fr => "Pas encore de série en cours.\n\
                 Commencez en enregistrant votre poids avec /weight 82,5 ou une séance avec /training.".to_string(),
        }
    }

    pub fn nudge_status(&self, enabled: bool, days: i64) -> String {
        match self.lang {
            Lang::En => {
                if enabled {
                    format!("Nudges: ON (after {days} days of inactivity)")
                } else {
                    "Nudges: OFF".to_string()
                }
            }
            Lang::Fr => {
                if enabled {
                    format!("Rappels : ACTIVÉS (après {days} jours d'inactivité)")
                } else {
                    "Rappels : DÉSACTIVÉS".to_string()
                }
            }
        }
    }

    pub fn nudge_enabled(&self, days: i64) -> String {
        match self.lang {
            Lang::En => format!("Nudges enabled (after {days} days of inactivity)."),
            Lang::Fr => format!("Rappels activés (après {days} jours d'inactivité)."),
        }
    }

    pub fn nudge_disabled(&self) -> &'static str {
        match self.lang {
            Lang::En => "Nudges disabled.",
            Lang::Fr => "Rappels désactivés.",
        }
    }

    pub fn nudge_threshold_set(&self, days: i64) -> String {
        match self.lang {
            Lang::En => format!("Nudge threshold set to {days} days of inactivity."),
            Lang::Fr => format!("Seuil de rappel défini à {days} jours d'inactivité."),
        }
    }

    pub fn nudge_usage(&self) -> &'static str {
        match self.lang {
            Lang::En => "Usage: /nudges [on|off|at N] — Manage inactivity reminders.",
            Lang::Fr => "Utilisation : /nudges [on|off|at N] — Gérer les rappels d'inactivité.",
        }
    }

    pub fn nudge_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update nudge settings.",
            Lang::Fr => "Échec de la mise à jour des paramètres de rappel.",
        }
    }

    pub fn sport_emoji_current(&self, emoji: &str) -> String {
        match self.lang {
            Lang::En => format!("Sport reaction emoji for this chat: {emoji}"),
            Lang::Fr => format!("Emoji de réaction sport pour ce chat : {emoji}"),
        }
    }

    pub fn sport_emoji_set(&self, emoji: &str) -> String {
        match self.lang {
            Lang::En => format!("Sport reaction emoji set to {emoji}"),
            Lang::Fr => format!("Emoji de réaction sport défini sur {emoji}"),
        }
    }

    pub fn sport_emoji_invalid(&self) -> &'static str {
        match self.lang {
            Lang::En => "Please provide a single emoji character (e.g., 🏆).",
            Lang::Fr => "Veuillez fournir un seul caractère emoji (par ex. 🏆).",
        }
    }

    pub fn sport_emoji_error(&self) -> &'static str {
        match self.lang {
            Lang::En => "Failed to update the sport reaction emoji.",
            Lang::Fr => "Échec de la mise à jour de l'emoji de réaction sport.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_cost_includes_window_cache_and_purpose() {
        let msg = Msg::new(Lang::En);
        let model_lines =
            ["  gemini-2.5-flash: 3 req, 12000 in / 1800 out, 4000 cache (~$0.0070)".into()];
        let kind_lines = [
            "  ask: 1 req (~$0.0070)".into(),
            "  compress: 1 req (~$0.0003)".into(),
            "  embed_query: 1 req (~$0.0000)".into(),
        ];
        let text = msg.user_cost(UserCostReport {
            telegram_user_id: 4242,
            window: "last 7 days",
            request_count: 3,
            prompt_tokens: 14012,
            completion_tokens: 2000,
            total_tokens: 16012,
            cached_tokens: 4000,
            cache_usd_saved: 0.0011,
            estimated_cost: 0.0073,
            model_lines: &model_lines,
            kind_lines: &kind_lines,
        });
        assert!(text.contains("AI spend for user 4242 (last 7 days)"));
        assert!(text.contains("By model:"));
        assert!(text.contains("By purpose:"));
        assert!(text.contains("Cache: 4000 tok, saved ~$0.0011"));
        assert!(text.contains("embed_query"));
        assert!(text.contains("compress"));
    }

    #[test]
    fn tool_loop_limit_is_localized() {
        let en = Msg::new(Lang::En).ai_tool_loop_limit();
        let fr = Msg::new(Lang::Fr).ai_tool_loop_limit();
        assert!(en.contains("stop repeating"));
        assert!(fr.contains("arrêter de relancer"));
        assert_ne!(en, fr);
        let en_writes = Msg::new(Lang::En).ai_tool_loop_limit_after_writes("remove_training #261");
        let fr_writes = Msg::new(Lang::Fr).ai_tool_loop_limit_after_writes("remove_training #261");
        assert!(en_writes.contains("remove_training #261"));
        assert!(en_writes.contains("already changed"));
        assert!(fr_writes.contains("remove_training #261"));
        assert!(fr_writes.contains("déjà modifié"));
        assert_ne!(en_writes, fr_writes);
    }
}
