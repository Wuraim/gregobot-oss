use serde::Serialize;
use teloxide::payloads::SendPhotoSetters;
use teloxide::prelude::Requester;

use super::context::ToolCtx;
use crate::db::strength_exercises::StrengthExercise;
use crate::productivity::domain::{TaskPriority, TaskSource, TaskStatus};

const TOOL_WEIGHT: &str = "get_weight_entries";
const TOOL_TRAINING: &str = "get_training_entries";
const TOOL_GET_TRAINING_ENTRY: &str = "get_training_entry";
const TOOL_GOALS: &str = "get_active_goals";
const TOOL_GROUP_TRAINING_STATS: &str = "get_group_training_stats";
const TOOL_TASKS: &str = "get_tasks";
const TOOL_REMINDERS: &str = "get_reminders";
const TOOL_CREATE_REMINDER: &str = "create_reminder";
const TOOL_UPDATE_REMINDER: &str = "update_reminder";
const TOOL_CANCEL_REMINDER: &str = "cancel_reminder";
const TOOL_CREATE_PROPOSAL: &str = "create_feature_proposal";
const TOOL_LIST_PROPOSALS: &str = "list_my_proposals";
const TOOL_REMOVE_PROPOSAL: &str = "remove_feature_proposal";
const TOOL_CREATE_TASK: &str = "create_task";
const TOOL_UPDATE_TASK: &str = "update_task";
const TOOL_REMOVE_TASK: &str = "remove_task";
const TOOL_MARK_TASK_DONE: &str = "mark_task_done";
const TOOL_MARK_TASK_CANCELLED: &str = "mark_task_cancelled";
const TOOL_SET_NUDGE_SETTINGS: &str = "set_nudge_settings";
const TOOL_LOG_WEIGHT: &str = "log_weight";
const TOOL_LOG_TRAINING: &str = "log_training";
const TOOL_UPDATE_TRAINING: &str = "update_training";
const TOOL_REMOVE_TRAINING: &str = "remove_training";
const TOOL_SHOW_WEIGHT_CHART: &str = "show_weight_chart";
const TOOL_SEARCH_FOODS: &str = "search_foods";
const TOOL_REGISTER_FOOD: &str = "register_food";
const TOOL_UPDATE_FOOD: &str = "update_food";
const TOOL_LOG_MEAL: &str = "log_meal";
const TOOL_GET_DAY_NUTRITION: &str = "get_day_nutrition";
const TOOL_SHOW_NUTRITION_TABLE: &str = "show_nutrition_table";
const TOOL_UPDATE_MEAL: &str = "update_meal";
const TOOL_REMOVE_MEAL: &str = "remove_meal";
const TOOL_PROPOSE_MEALS: &str = "propose_meals";
const TOOL_REMEMBER_FOOD_PREFERENCE: &str = "remember_food_preference";
const TOOL_SEND_COACH_IMAGE: &str = "send_coach_image";
const TOOL_GET_IMAGE_QUOTA: &str = "get_image_quota";
const TOOL_SEARCH_PHOTOS: &str = "search_photos";
const TOOL_LIST_CAPABILITIES: &str = "list_capabilities";

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// Rich multilingual description used for semantic tool selection.
    /// Not sent to the LLM (it is skipped during serialization).
    #[serde(skip)]
    pub embed_text: &'static str,
}

pub fn all_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: TOOL_WEIGHT.into(),
            description: "Get the user's recent weight entries (kg), most recent first".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Max entries to return (default 10, max 30)"
                        }
                    },
                    "required": []
                }),
            embed_text: "get_weight_entries: Get the user's recent weight entries in kilograms. Use when the user asks about: my weight, how much do I weigh, weight history, weight loss, weight gain, latest weight. Prefer show_weight_chart when they want a graph.",
        },
        ToolDef {
            name: TOOL_TRAINING.into(),
            description: "Get the user's recent training/exercise sessions".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Max entries to return (default 10, max 30)"
                        }
                    },
                    "required": []
                }),
            embed_text: "get_training_entries: Get the user's recent training or exercise sessions with activity type, distance, duration, and gym sets when stored. Use when the user asks about: my workouts, training history, last run, gym session.",
        },
        ToolDef {
            name: TOOL_GET_TRAINING_ENTRY.into(),
            description: "Get one training session by ID as full JSON plus a formatted card (owner or admin)".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "training_id": {
                            "type": "integer",
                            "description": "ID of the training entry to show"
                        }
                    },
                    "required": ["training_id"]
                }),
            embed_text: "get_training_entry: Show this workout in full detail: session card, carnet, séries, last session details, détail séance, fiche entraînement. Use when the user asks for one specific session by id, not a list of recent workouts.",
        },
        ToolDef {
            name: TOOL_GOALS.into(),
            description: "Get the user's active fitness goals with current progress".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            embed_text: "get_active_goals: Get the user's active fitness goals and progress. Use when the user asks about: my goals, mes objectifs, goal progress, poids objectif, training frequency goal.",
        },
        ToolDef {
            name: TOOL_GROUP_TRAINING_STATS.into(),
            description: "Group training session ranking for this chat (last calendar month, last 4 weeks, or last 3 months). Session counts only — not weight.".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "period": {
                            "type": "string",
                            "enum": ["last", "4w", "3m"],
                            "description": "Window: last = previous calendar month, 4w = last 4 weeks, 3m = last 3 months (default last)"
                        }
                    },
                    "required": []
                }),
            embed_text: "get_group_training_stats: Group training ranking and session counts for this chat. Use when the user asks about: classement, who is leading, rivalité, monthly graph, séances du groupe, who trained more, ranking among friends.",
        },
        ToolDef {
            name: TOOL_TASKS.into(),
            description: "Get the user's tasks, optionally filtered by status".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["todo", "doing", "done", "cancelled"],
                            "description": "Filter by task status"
                        }
                    },
                    "required": []
                }),
            embed_text: "get_tasks: Get the user's tasks or todos, optionally filtered by status. Use when the user asks about: my tasks, mes tâches, todo list, what do I have to do, pending tasks.",
        },
        ToolDef {
            name: TOOL_REMINDERS.into(),
            description: "Get the user's pending reminders, including their message when available".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            embed_text: "get_reminders: Get the user's pending reminders. Use when the user asks about: my reminders, mes rappels, pending reminders, what should I remember.",
        },
        ToolDef {
            name: TOOL_CREATE_REMINDER.into(),
            description: "Create a reminder that will fire at a specified time, optionally repeating".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "scheduled_at": {
                            "type": "string",
                            "description": "When the reminder should fire, in RFC3339 format (e.g. 2024-01-15T14:00:00Z)"
                        },
                        "message": {
                            "type": "string",
                            "description": "Reminder message text"
                        },
                        "recurrence": {
                            "type": "string",
                            "enum": ["none", "daily", "weekly", "monthly"],
                            "description": "How often the reminder repeats. Default: none."
                        }
                    },
                    "required": ["scheduled_at"]
                }),
            embed_text: "create_reminder: Create a reminder that fires at a specified time, optionally repeating. Use when the user says: remind me to, rappelle-moi de, set a reminder, create a reminder.",
        },
        ToolDef {
            name: TOOL_UPDATE_REMINDER.into(),
            description: "Move an existing reminder to a new scheduled time".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reminder_id": {
                            "type": "integer",
                            "description": "ID of the reminder to move"
                        },
                        "scheduled_at": {
                            "type": "string",
                            "description": "New fire time in RFC3339 format"
                        }
                    },
                    "required": ["reminder_id", "scheduled_at"]
                }),
            embed_text: "update_reminder: Move or change an existing reminder. Use when the user wants to: reschedule a reminder, décaler un rappel, change reminder time, move reminder.",
        },
        ToolDef {
            name: TOOL_CANCEL_REMINDER.into(),
            description: "Cancel a pending reminder by ID".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reminder_id": {
                            "type": "integer",
                            "description": "ID of the reminder to cancel"
                        }
                    },
                    "required": ["reminder_id"]
                }                ),
            embed_text: "cancel_reminder: Cancel a pending reminder by ID. Use when the user wants to: cancel a reminder, annuler un rappel, delete a reminder.",
        },
        ToolDef {
            name: TOOL_CREATE_PROPOSAL.into(),
            description: "Create a new feature proposal (request for a new bot capability) on behalf of the user".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Short title summarizing the feature request"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional longer description of what the user wants"
                        }
                    },
                    "required": ["title"]
                }),
            embed_text: "create_feature_proposal: Create a new feature proposal or idea for the bot. Use when the user suggests: a feature, une idée, it would be nice if, the bot should, improvement suggestion.",
        },
        ToolDef {
            name: TOOL_LIST_PROPOSALS.into(),
            description: "List feature proposals. Non-admins see only their own; admins see everyone's".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            embed_text: "list_my_proposals: List feature proposals. Non-admins: own only. Admins: all users' proposals. Use when the user asks about: my proposals, mes suggestions, what did I propose, feature requests, all proposals.",
        },
        ToolDef {
            name: TOOL_REMOVE_PROPOSAL.into(),
            description: "Remove a feature proposal by ID. Non-admins can only remove their own; admins can remove any".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "proposal_id": {
                            "type": "integer",
                            "description": "ID of the proposal to remove"
                        }
                    },
                    "required": ["proposal_id"]
                }),
            embed_text: "remove_feature_proposal: Remove/delete a feature proposal by ID. Non-admins: own only. Admins: any. Use when the user wants to: delete a proposal, supprimer une proposition, remove a feature request, cancel a suggestion.",
        },
        ToolDef {
            name: TOOL_CREATE_TASK.into(),
            description: "Create a new task/todo for the user".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Task title"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "description": "Task priority (default: medium)"
                        },
                        "due_at": {
                            "type": "string",
                            "description": "Optional due date in RFC3339 format (e.g. 2024-01-15T14:00:00Z)"
                        },
                        "tags": {
                            "type": "string",
                            "description": "Optional comma-separated tags"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional task description"
                        }
                    },
                    "required": ["title"]
                }),
            embed_text: "create_task: Create a new task or todo for the user. Use when the user says: add a task, ajouter une tâche, create a todo, I need to do, remember to.",
        },
        ToolDef {
            name: TOOL_UPDATE_TASK.into(),
            description: "Update one or more fields of an existing task; omitted fields stay unchanged".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "integer",
                            "description": "ID of the task to update"
                        },
                        "title": {
                            "type": "string",
                            "description": "New title"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "description": "New priority"
                        },
                        "due_at": {
                            "type": "string",
                            "description": "New due date in RFC3339 format"
                        },
                        "tags": {
                            "type": "string",
                            "description": "New comma-separated tags"
                        },
                        "description": {
                            "type": "string",
                            "description": "New description"
                        }
                    },
                    "required": ["task_id"]
                }),
            embed_text: "update_task: Update an existing task (title, priority, due date, tags, description). Use when the user wants to: change a task, modifier une tâche, update todo, reschedule task.",
        },
        ToolDef {
            name: TOOL_REMOVE_TASK.into(),
            description: "Remove a task permanently by ID".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "integer",
                            "description": "ID of the task to remove"
                        }
                    },
                    "required": ["task_id"]
                }),
            embed_text: "remove_task: Remove or delete a task permanently by ID. Use when the user wants to delete a task, remove a todo, or drop a task from their list.",
        },
        ToolDef {
            name: TOOL_MARK_TASK_DONE.into(),
            description: "Mark a task as completed/done".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "integer",
                            "description": "ID of the task to mark done"
                        }
                    },
                    "required": ["task_id"]
                }),
            embed_text: "mark_task_done: Mark a task as done by ID. Use when the user says: I did a task, task done, tâche terminée, mark as completed, done with todo.",
        },
        ToolDef {
            name: TOOL_MARK_TASK_CANCELLED.into(),
            description: "Cancel a task (mark as cancelled, not done)".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "integer",
                            "description": "ID of the task to cancel"
                        }
                    },
                    "required": ["task_id"]
                }),
            embed_text: "mark_task_cancelled: Mark a task as cancelled by ID. Use when the user says: cancel a task, annuler une tâche, task no longer needed.",
        },
        ToolDef {
            name: TOOL_SET_NUDGE_SETTINGS.into(),
            description: "Configure inactivity nudge reminders for the user".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "enabled": {
                            "type": "boolean",
                            "description": "Enable or disable nudges"
                        },
                        "inactivity_days": {
                            "type": "integer",
                            "description": "Number of inactive days before a nudge is sent (e.g. 3, 5, 7)"
                        }
                    },
                    "required": []
                }),
            embed_text: "set_nudge_settings: Configure inactivity nudges (enable/disable and inactivity threshold in days). Use when the user asks about: nudges, rappels d'inactivité, motivation reminders, change nudge settings.",
        },
        ToolDef {
            name: TOOL_LOG_WEIGHT.into(),
            description: "Log a weight measurement for the user (kg).".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "weight_kg": {
                            "type": "number",
                            "description": "Body weight in kilograms, e.g. 82.5. Must be a realistic human body weight (20–400). Do not pass gym bar or machine loads."
                        },
                        "recorded_at": {
                            "type": "string",
                            "description": "Optional timestamp. Can be RFC3339 (e.g. 2024-01-15T14:00:00Z) or a relative expression like 'yesterday', 'this morning', 'hier', 'ce soir'. Default: now."
                        }
                    },
                    "required": ["weight_kg"]
                }),
            embed_text: "log_weight: Log the user's weight in kilograms. Use when the user says: I weigh, je pèse, my weight is, poids du jour, recorded weight.",
        },
        ToolDef {
            name: TOOL_LOG_TRAINING.into(),
            description: "Log a training/exercise session for the user. Use activity + endurance stats for runs/rides, strength_sets for gym, both for CrossFit. When a workout screenshot is attached, extract visible numbers (distance in meters, duration in seconds). If this session was just logged, do not insert again — the tool returns duplicate=true and you should update_training instead.".into(),
            parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "Short human summary of the session"
                        },
                        "activity": {
                            "type": "string",
                            "enum": ["run", "ride", "walk", "hike", "swim", "gym", "crossfit", "other"],
                            "description": "Activity type. run/ride/walk/hike/swim = endurance; gym = strength; crossfit = mixed."
                        },
                        "recorded_at": {
                            "type": "string",
                            "description": "Optional timestamp. Can be RFC3339 (e.g. 2024-01-15T14:00:00Z) or a relative expression like 'yesterday', 'this morning', 'hier', 'ce soir'. Default: now."
                        },
                        "endurance": {
                            "type": "object",
                            "description": "Endurance stats (run, ride, walk, hike, swim, crossfit). Omit unknown fields.",
                            "properties": {
                                "duration_sec": { "type": "integer", "description": "Duration in seconds" },
                                "distance_m": { "type": "number", "description": "Distance in meters (8.2 km = 8200)" },
                                "elevation_m": { "type": "number", "description": "Elevation gain in meters" },
                                "avg_hr": { "type": "integer", "description": "Average heart rate" },
                                "calories": { "type": "integer", "description": "Calories" }
                            }
                        },
                        "strength_sets": {
                            "type": "array",
                            "description": "Gym or CrossFit sets. exercise is a catalog slug (bench_press, squat, …).",
                            "items": strength_set_item_schema()
                        }
                    },
                    "required": ["description"]
                }),
            embed_text: "log_training: Log a training or exercise session. Use for runs, rides, gym, CrossFit, Strava screenshots. Fill endurance distance_m and duration_sec when known; fill strength_sets with catalog exercise slugs (bench_press, squat, dumbbell_biceps_curl) for gym.",
        },
        ToolDef {
            name: TOOL_UPDATE_TRAINING.into(),
            description: "Update a training entry by ID (description, date, activity, endurance stats, and/or strength sets). Sportive users may only edit their own; admins may edit any.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "training_id": {
                        "type": "integer",
                        "description": "ID of the training entry to update"
                    },
                    "description": {
                        "type": "string",
                        "description": "New description for the session"
                    },
                    "recorded_at": {
                        "type": "string",
                        "description": "New date/time for the session (user timezone or ISO)"
                    },
                    "activity": {
                        "type": "string",
                        "enum": ["run", "ride", "walk", "hike", "swim", "gym", "crossfit", "other"]
                    },
                    "endurance": {
                        "type": "object",
                        "properties": {
                            "duration_sec": { "type": "integer" },
                            "distance_m": { "type": "number" },
                            "elevation_m": { "type": "number" },
                            "avg_hr": { "type": "integer" },
                            "calories": { "type": "integer" }
                        }
                    },
                    "strength_sets": {
                        "type": "array",
                        "description": "Replaces all stored sets. exercise is a catalog slug.",
                        "items": strength_set_item_schema()
                    }
                },
                "required": ["training_id"]
            }),
            embed_text: "update_training: Edit an existing training session by ID, including activity type, distance, duration, or gym sets.",
        },
        ToolDef {
            name: TOOL_REMOVE_TRAINING.into(),
            description: "Delete a training entry by ID. Sportive users may only delete their own; admins may delete any.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "training_id": {
                        "type": "integer",
                        "description": "ID of the training entry to delete"
                    }
                },
                "required": ["training_id"]
            }),
            embed_text: "remove_training: Delete a training or workout session by ID. Use when the user wants to: delete a session, remove a workout, cancel a logged training, erase training entry.",
        },
        ToolDef {
            name: TOOL_SHOW_WEIGHT_CHART.into(),
            description: "Generate and send a PNG weight history chart for the current user (30/90 days or all). Use when they ask for a weight graph/curve.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "days": {
                        "type": "string",
                        "description": "Time window: \"30\", \"90\" (default), or \"all\"",
                        "enum": ["30", "90", "all"]
                    }
                }
            }),
            embed_text: "show_weight_chart: Send a visual weight history chart/graph as an image. Use when the user asks about: weight chart, weight graph, show my weight trend, plot my weight.",
        },
        ToolDef {
            name: TOOL_SEARCH_FOODS.into(),
            description: "Search the user's personal food catalog (name, brand, per-100g macros, default serving). Use before logging a meal when they mention a known product.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Food name or brand. Empty lists the whole catalog."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max foods to return (default 20, max 40)"
                    }
                }
            }),
            embed_text: "search_foods: Look up a saved food product, whey, oats, macros per 100g, personal food catalog, what is in my whey.",
        },
        ToolDef {
            name: TOOL_REGISTER_FOOD.into(),
            description: "Save a product in the user's food catalog. Prefer per-100g nutrients; if the label is per serving, send serving macros plus serving_qty in grams.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Product name" },
                    "brand": { "type": "string" },
                    "barcode": { "type": "string" },
                    "notes": { "type": "string" },
                    "kcal_per_100": { "type": "number", "description": "kcal per 100g (or 100ml)" },
                    "protein_g_per_100": { "type": "number" },
                    "carbs_g_per_100": { "type": "number" },
                    "fat_g_per_100": { "type": "number" },
                    "kcal": { "type": "number", "description": "kcal per serving (converted using serving_qty)" },
                    "protein_g": { "type": "number" },
                    "carbs_g": { "type": "number" },
                    "fat_g": { "type": "number" },
                    "serving_qty": { "type": "number", "description": "Default serving size in grams or ml (default 100)" },
                    "serving_unit": { "type": "string", "enum": ["g", "ml", "serving", "piece"] },
                    "serving_label": { "type": "string", "description": "e.g. 1 scoop" },
                    "source": { "type": "string", "enum": ["manual", "photo", "api"] }
                },
                "required": ["name"]
            }),
            embed_text: "register_food: Save a food product, whey label, nutrition facts, macros per 100g, add food to catalog from a photo of the label.",
        },
        ToolDef {
            name: TOOL_UPDATE_FOOD.into(),
            description: "Update a saved food product (macros, serving, name, brand).".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "food_id": { "type": "integer" },
                    "name": { "type": "string" },
                    "brand": { "type": "string" },
                    "barcode": { "type": "string" },
                    "notes": { "type": "string" },
                    "kcal_per_100": { "type": "number" },
                    "protein_g_per_100": { "type": "number" },
                    "carbs_g_per_100": { "type": "number" },
                    "fat_g_per_100": { "type": "number" },
                    "serving_qty": { "type": "number" },
                    "serving_unit": { "type": "string", "enum": ["g", "ml", "serving", "piece"] },
                    "serving_label": { "type": "string" }
                },
                "required": ["food_id"]
            }),
            embed_text: "update_food: Correct a saved food product macros, serving size, brand, or name in the catalog.",
        },
        ToolDef {
            name: TOOL_LOG_MEAL.into(),
            description: "Log a meal with one or more items. Prefer catalog foods (food_id or name). For unknown foods, pass estimated macros and source=estimated.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["breakfast", "lunch", "dinner", "snack", "other"]
                    },
                    "label": { "type": "string", "description": "Free label, e.g. post-workout" },
                    "eaten_at": { "type": "string", "description": "When eaten (RFC3339 or yesterday/today)" },
                    "notes": { "type": "string" },
                    "items": {
                        "type": "array",
                        "items": meal_item_schema()
                    }
                },
                "required": ["kind", "items"]
            }),
            embed_text: "log_meal: Log breakfast lunch dinner snack calories protein carbs fat, I ate whey, food diary, what I consumed.",
        },
        ToolDef {
            name: TOOL_GET_DAY_NUTRITION.into(),
            description: "Get all meals, items, macros and day totals for a local date (default today). Use to answer what the user ate.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "date": {
                        "type": "string",
                        "description": "today, yesterday, or YYYY-MM-DD (user timezone)"
                    }
                }
            }),
            embed_text: "get_day_nutrition: What did I eat today, calories today, protein today, macros for the day, food diary, meals logged.",
        },
        ToolDef {
            name: TOOL_SHOW_NUTRITION_TABLE.into(),
            description: "Send a PNG nutrition table for a day (private chats only). Use when the user wants to see meals and macros visually.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "date": {
                        "type": "string",
                        "description": "today, yesterday, or YYYY-MM-DD (user timezone)"
                    }
                }
            }),
            embed_text: "show_nutrition_table: Show my nutrition table, calories table, meals chart, daily macros image.",
        },
        ToolDef {
            name: TOOL_UPDATE_MEAL.into(),
            description: "Update a logged meal (kind, time, notes) and optionally replace its items.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "meal_id": { "type": "integer" },
                    "kind": {
                        "type": "string",
                        "enum": ["breakfast", "lunch", "dinner", "snack", "other"]
                    },
                    "label": { "type": "string" },
                    "eaten_at": { "type": "string" },
                    "notes": { "type": "string" },
                    "items": {
                        "type": "array",
                        "items": meal_item_schema()
                    }
                },
                "required": ["meal_id"]
            }),
            embed_text: "update_meal: Edit a meal, change quantity, it was two scoops not one, fix food diary entry.",
        },
        ToolDef {
            name: TOOL_REMOVE_MEAL.into(),
            description: "Delete a logged meal by ID.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "meal_id": { "type": "integer" }
                },
                "required": ["meal_id"]
            }),
            embed_text: "remove_meal: Delete a meal from the food diary, remove breakfast, undo food log.",
        },
        ToolDef {
            name: TOOL_PROPOSE_MEALS.into(),
            description: "Propose 2–3 meals from the user's current craving (envie), personal catalog, today's diary, and food preferences. Does not log anything.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "envie": {
                        "type": "string",
                        "description": "What they feel like eating: salty, quick, pasta, leftover macros, hungry, craving."
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["breakfast", "lunch", "dinner", "snack", "other"]
                    },
                    "minutes": {
                        "type": "integer",
                        "description": "How long they will cook or prep"
                    }
                },
                "required": ["envie"]
            }),
            embed_text: "propose_meals: Craving, what should I eat, hungry, leftover macros, meal idea, dinner idea, snack idea, what to eat after training.",
        },
        ToolDef {
            name: TOOL_REMEMBER_FOOD_PREFERENCE.into(),
            description: "Save a lasting food preference: allergy, dislike, like, or habit. Use for never-eat-X, I love Y, weekday staple. Does not log a meal.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["allergy", "dislike", "like", "habit"]
                    },
                    "text": {
                        "type": "string",
                        "description": "Short preference, e.g. lactose, tuna, salty post-ride food"
                    },
                    "food_id": {
                        "type": "integer",
                        "description": "Catalog food id when the preference is about a known product"
                    }
                },
                "required": ["kind", "text"]
            }),
            embed_text: "remember_food_preference: Allergy, dislike, I hate tuna, I love salty food, no lactose, staple habit, never eat this.",
        },
        ToolDef {
            name: TOOL_SEND_COACH_IMAGE.into(),
            description: "Rarely generate or edit a ceremonial coach image and send it to the chat. Use only when a real fact deserves a victory poster, good-natured sarcasm, or a remix of a screenshot or label from this turn. Never for body/physique photos. Daily quota is small.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["victory", "sarcasm", "edit"],
                        "description": "victory = ceremonial poster; sarcasm = good-natured mock-heroic; edit = remix a screenshot or label from this turn — never a body/physique photo"
                    },
                    "scene": {
                        "type": "string",
                        "description": "Short English visual prompt. Stick to the real fact. Do not invent numbers."
                    },
                    "caption": {
                        "type": "string",
                        "description": "1–2 coach sentences in the user's language (not a novel)"
                    },
                    "use_user_photo": {
                        "type": "boolean",
                        "description": "If true, edit the photo attached to this turn. Required for kind=edit unless photo_id is set."
                    },
                    "photo_id": {
                        "type": "integer",
                        "description": "Stored photo_id from search_photos to edit instead of this turn's photo"
                    }
                },
                "required": ["kind", "scene"]
            }),
            embed_text: "send_coach_image: Rarely generate a ceremonial sports poster, sarcastic edit, screenshot remix, or celebration image. Use only for a real victory, PR, goal reached, or good-natured sarcasm — not every chat turn. Never for body or physique photos. Prefer get_image_quota when the user only asks about the image limit.",
        },
        ToolDef {
            name: TOOL_GET_IMAGE_QUOTA.into(),
            description: "Get this user's coach-image quota for the current UTC day (max, used, remaining). Use when they ask how many images are left, the daily limit, or whether you can still generate a picture.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
            embed_text: "get_image_quota: Remaining coach images today, daily image limit, how many pictures left, quota images, limite images, combien d'images, can you still generate an image.",
        },
        ToolDef {
            name: TOOL_SEARCH_PHOTOS.into(),
            description: "Search this user's stored screenshots, training logs, and nutrition labels by meaning. Returns photo_id, taken_at, caption — not pixels. Optionally send a match to the chat. Do not use this to track physique or to ask for a body photo. Private chats search private photos; group chats search this chat only.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to find (e.g. Strava run, squat screenshot). Optional if this turn already has a prompt embedding."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max hits (default 5, max 10)"
                    },
                    "send": {
                        "type": "boolean",
                        "description": "If true, send the best match (or photo_id) to the chat"
                    },
                    "photo_id": {
                        "type": "integer",
                        "description": "Send or inspect this stored photo_id from a previous search"
                    }
                }
            }),
            embed_text: "search_photos: Find a past screenshot, Strava, Garmin, gym log, or nutrition-label photo the user sent. Use when they ask: that screenshot, la capture de mardi, Strava photo, Garmin, food label. Not for body, selfie, physique, or progress pictures.",
        },
        ToolDef {
            name: TOOL_LIST_CAPABILITIES.into(),
            description: "List all bot capabilities (full tool catalog). Use when the user asks what you can do, your features, or a presentation of available tools. Note: other tools attached this turn were pre-selected by relevance to the message; this returns the complete set for presentation.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
            embed_text: "list_capabilities: Present what the bot can do. Use when the user asks: what can you do, que peux-tu faire, tes capacités, capabilities, features, fonctionnalités, présentation, outils disponibles, what are your tools, aide fonctionnalités.",
        },
    ]
}

/// Compact catalog of every tool except `list_capabilities` itself.
pub fn capability_catalog() -> String {
    all_tools()
        .into_iter()
        .filter(|t| t.name != TOOL_LIST_CAPABILITIES)
        .map(|t| format!("{} — {}", t.name, t.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return a subset of tools by name. Names that do not exist are ignored.
pub fn tools_by_names(names: &[String]) -> Vec<ToolDef> {
    let name_set: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
    all_tools()
        .into_iter()
        .filter(|t| name_set.contains(t.name.as_str()))
        .collect()
}

pub fn unknown_tool_response(name: &str, available_tools: &[String]) -> String {
    serde_json::json!({
        "error": format!("Unknown tool: {name}"),
        "hint": "Call only tools declared in function_declarations for this turn.",
        "available_tools": available_tools,
    })
    .to_string()
}

pub fn is_unknown_tool_response(result: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.starts_with("Unknown tool:"))
        })
        .unwrap_or(false)
}

pub async fn execute_tool(
    ctx: &ToolCtx<'_>,
    name: &str,
    args: &serde_json::Value,
    available_tools: &[String],
) -> String {
    match name {
        TOOL_WEIGHT => execute_weight(ctx, args).await,
        TOOL_TRAINING => execute_training(ctx, args).await,
        TOOL_GET_TRAINING_ENTRY => execute_get_training_entry(ctx, args).await,
        TOOL_GOALS => execute_goals(ctx).await,
        TOOL_GROUP_TRAINING_STATS => execute_group_training_stats(ctx, args).await,
        TOOL_TASKS => execute_tasks(ctx, args).await,
        TOOL_REMINDERS => execute_reminders(ctx).await,
        TOOL_CREATE_REMINDER => execute_create_reminder(ctx, args).await,
        TOOL_UPDATE_REMINDER => execute_update_reminder(ctx, args).await,
        TOOL_CANCEL_REMINDER => execute_cancel_reminder(ctx, args).await,
        TOOL_CREATE_PROPOSAL => execute_create_proposal(ctx, args).await,
        TOOL_LIST_PROPOSALS => execute_list_proposals(ctx).await,
        TOOL_REMOVE_PROPOSAL => execute_remove_proposal(ctx, args).await,
        TOOL_CREATE_TASK => execute_create_task(ctx, args).await,
        TOOL_UPDATE_TASK => execute_update_task(ctx, args).await,
        TOOL_REMOVE_TASK => execute_delete_task(ctx, args).await,
        TOOL_MARK_TASK_DONE => execute_mark_task_done(ctx, args).await,
        TOOL_MARK_TASK_CANCELLED => execute_mark_task_cancelled(ctx, args).await,
        TOOL_SET_NUDGE_SETTINGS => execute_set_nudge_settings(ctx, args).await,
        TOOL_LOG_WEIGHT => execute_log_weight(ctx, args).await,
        TOOL_LOG_TRAINING => execute_log_training(ctx, args).await,
        TOOL_UPDATE_TRAINING => execute_update_training(ctx, args).await,
        TOOL_REMOVE_TRAINING => execute_remove_training(ctx, args).await,
        TOOL_SHOW_WEIGHT_CHART => execute_show_weight_chart(ctx, args).await,
        TOOL_SEARCH_FOODS => execute_search_foods(ctx, args).await,
        TOOL_REGISTER_FOOD => execute_register_food(ctx, args).await,
        TOOL_UPDATE_FOOD => execute_update_food(ctx, args).await,
        TOOL_LOG_MEAL => execute_log_meal(ctx, args).await,
        TOOL_GET_DAY_NUTRITION => execute_get_day_nutrition(ctx, args).await,
        TOOL_SHOW_NUTRITION_TABLE => execute_show_nutrition_table(ctx, args).await,
        TOOL_UPDATE_MEAL => execute_update_meal(ctx, args).await,
        TOOL_REMOVE_MEAL => execute_remove_meal(ctx, args).await,
        TOOL_PROPOSE_MEALS => execute_propose_meals(ctx, args).await,
        TOOL_REMEMBER_FOOD_PREFERENCE => execute_remember_food_preference(ctx, args).await,
        TOOL_SEND_COACH_IMAGE => execute_send_coach_image(ctx, args).await,
        TOOL_GET_IMAGE_QUOTA => execute_get_image_quota(ctx).await,
        TOOL_SEARCH_PHOTOS => execute_search_photos(ctx, args).await,
        TOOL_LIST_CAPABILITIES => execute_list_capabilities().await,
        _ => unknown_tool_response(name, available_tools),
    }
}

async fn execute_list_capabilities() -> String {
    serde_json::json!({
        "note": "Tools attached to this turn were chosen by embedding similarity to the user message. Below is the full catalog for presentation; only attached tools are callable right now.",
        "capabilities": capability_catalog(),
    })
    .to_string()
}

async fn execute_weight(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(10)
        .clamp(1, 30);

    match crate::db::weight::list_for_user(ctx.pool, ctx.user_id).await {
        Ok(entries) => {
            let data: Vec<_> = entries
                .iter()
                .take(limit as usize)
                .map(|e| {
                    serde_json::json!({
                        "date": e.recorded_at.format("%Y-%m-%d").to_string(),
                        "weight_kg": e.weight_kg,
                    })
                })
                .collect();
            serde_json::to_string(&serde_json::json!({"weight_entries": data})).unwrap_or_default()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to fetch weight entries: {e}")}).to_string()
        }
    }
}

async fn execute_training(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(10)
        .clamp(1, 30);

    match crate::db::training::list_for_user(ctx.pool, ctx.user_id).await {
        Ok(entries) => {
            let data: Vec<_> = entries
                .iter()
                .take(limit as usize)
                .map(|e| {
                    serde_json::json!({
                        "id": e.id,
                        "date": e.recorded_at.format("%Y-%m-%d").to_string(),
                        "description": e.description,
                        "activity": e.activity_slug,
                        "endurance": e.endurance,
                        "strength_sets": strength_sets_json(&e.strength_sets),
                        "summary": crate::db::training::summary_line(e),
                    })
                })
                .collect();
            serde_json::to_string(&serde_json::json!({"training_entries": data}))
                .unwrap_or_default()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to fetch training entries: {e}")})
            .to_string(),
    }
}

async fn execute_get_training_entry(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let training_id = match args.get("training_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "training_id is required"}).to_string(),
    };

    match crate::db::training::find_by_id(ctx.pool, training_id).await {
        Ok(Some(entry)) if entry.user_id == ctx.user_id || ctx.is_admin => {
            serde_json::to_string(&training_entry_card_json(&entry)).unwrap_or_default()
        }
        Ok(Some(_)) | Ok(None) => serde_json::json!({
            "error": "Training not found, or you are not allowed to view it"
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to fetch training entry: {e}")}).to_string()
        }
    }
}

async fn execute_goals(ctx: &ToolCtx<'_>) -> String {
    match crate::db::goals::list_active(ctx.pool, ctx.user_id).await {
        Ok(goals) => {
            let mut data = Vec::new();
            for g in goals {
                let kind = match crate::db::goals::GoalType::from_str(&g.goal_type) {
                    Some(crate::db::goals::GoalType::Weight) => "weight",
                    Some(crate::db::goals::GoalType::TrainingFrequency) => "training_frequency",
                    None => "unknown",
                };
                let current_value = crate::db::goals::GoalType::from_str(&g.goal_type)
                    .map(|gt| crate::db::goals::current_value_for(ctx.pool, ctx.user_id, gt));
                let current_value = match current_value {
                    Some(fut) => fut.await.ok().flatten(),
                    None => None,
                };
                data.push(serde_json::json!({
                    "id": g.id,
                    "type": kind,
                    "target_value": g.target_value,
                    "current_value": current_value,
                    "is_active": g.is_active,
                    "created_at": g.created_at,
                }));
            }
            serde_json::to_string(&serde_json::json!({"goals": data})).unwrap_or_default()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to fetch goals: {e}")}).to_string(),
    }
}

fn group_stats_dm_error() -> String {
    serde_json::json!({
        "error": "This chat is not the group. Do not invent a group session total. Tell the user to ask in the group."
    })
    .to_string()
}

async fn execute_group_training_stats(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let Some(chat_id) = ctx.chat_id.filter(|id| *id < 0) else {
        return group_stats_dm_error();
    };

    match crate::db::group_members::list_for_chat(ctx.pool, chat_id).await {
        Ok(members) if members.is_empty() => {
            return serde_json::json!({
                "error": "No members are tracked in this group yet. Do not invent a session count."
            })
            .to_string();
        }
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to list group members: {e}")})
                .to_string();
        }
        Ok(_) => {}
    }

    let period = args
        .get("period")
        .and_then(|v| v.as_str())
        .unwrap_or("last");
    let (start, end, label) = match group_stats_window(period) {
        Some(w) => w,
        None => {
            return serde_json::json!({
                "error": "Invalid period. Use \"last\", \"4w\", or \"3m\"."
            })
            .to_string();
        }
    };

    match crate::db::training::sessions_by_group_member(
        ctx.pool,
        chat_id,
        &start.to_rfc3339(),
        &end.to_rfc3339(),
    )
    .await
    {
        Ok(stats) => {
            let ranking: Vec<_> = stats
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.display_name(),
                        "sessions": s.sessions,
                    })
                })
                .collect();
            serde_json::to_string(&serde_json::json!({
                "period": period,
                "label": label,
                "ranking": ranking,
            }))
            .unwrap_or_default()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to fetch group training stats: {e}")})
                .to_string()
        }
    }
}

fn group_stats_window(
    period: &str,
) -> Option<(
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    String,
)> {
    match period {
        "last" => Some(crate::bot::monthly_graph::previous_month_window()),
        "4w" => Some(crate::bot::monthly_graph::last_weeks_window(4)),
        "3m" => Some(crate::bot::monthly_graph::last_months_window(3)),
        _ => None,
    }
}

async fn execute_tasks(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let status_filter = status.and_then(|s| TaskStatus::from_str(&s));
    match crate::productivity::task_service::list_tasks(ctx.pool, ctx.user_id, status_filter).await
    {
        Ok(tasks) => {
            let data: Vec<_> = tasks
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "title": t.title,
                        "status": t.status.as_str(),
                        "priority": t.priority.as_str(),
                        "due_at": t.due_at.map(|d| d.format("%Y-%m-%d").to_string()),
                    })
                })
                .collect();
            serde_json::to_string(&serde_json::json!({"tasks": data})).unwrap_or_default()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to fetch tasks: {e}")}).to_string(),
    }
}

async fn execute_create_task(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let title = match args.get("title").and_then(|v| v.as_str()) {
        Some(t) if !t.trim().is_empty() => t.to_string(),
        _ => {
            return serde_json::json!({"error": "title is required and must be non-empty"})
                .to_string();
        }
    };

    let priority = args
        .get("priority")
        .and_then(|v| v.as_str())
        .and_then(TaskPriority::from_str)
        .unwrap_or(TaskPriority::Medium);

    let due_at = parse_datetime_arg(args, "due_at", ctx.timezone);

    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_str())
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match crate::productivity::task_service::create_task(
        ctx.pool,
        crate::productivity::task_service::CreateTaskInput {
            user_id: ctx.user_id,
            title,
            description,
            priority,
            due_at,
            tags,
            source: TaskSource::AiPlanner,
        },
    )
    .await
    {
        Ok(task) => serde_json::json!({
            "success": true,
            "task": {
                "id": task.id,
                "title": task.title,
                "status": task.status.as_str(),
                "priority": task.priority.as_str(),
                "due_at": task.due_at.map(|d| d.to_rfc3339()),
            }
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to create task: {e}")}).to_string(),
    }
}

async fn execute_update_task(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let task_id = match args.get("task_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "task_id is required"}).to_string(),
    };

    let updates = crate::productivity::task_service::UpdateTask {
        title: args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        description: args
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        priority: args
            .get("priority")
            .and_then(|v| v.as_str())
            .and_then(TaskPriority::from_str),
        due_at: match args.get("due_at").and_then(|v| v.as_str()) {
            Some("") => Some(None),
            Some(_) => parse_datetime_arg(args, "due_at", ctx.timezone).map(Some),
            None => None,
        },
        tags: args.get("tags").and_then(|v| v.as_str()).map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        }),
    };

    match crate::productivity::task_service::update_task(ctx.pool, task_id, ctx.user_id, updates)
        .await
    {
        Ok(task) => serde_json::json!({
            "success": true,
            "task": {
                "id": task.id,
                "title": task.title,
                "status": task.status.as_str(),
                "priority": task.priority.as_str(),
                "due_at": task.due_at.map(|d| d.to_rfc3339()),
            }
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to update task: {e}")}).to_string(),
    }
}

async fn execute_delete_task(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let task_id = match args.get("task_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "task_id is required"}).to_string(),
    };

    match crate::productivity::task_service::delete_task(ctx.pool, task_id, ctx.user_id).await {
        Ok(true) => serde_json::json!({"success": true, "message": "Task deleted"}).to_string(),
        Ok(false) => serde_json::json!({"error": "Task not found or not yours"}).to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to delete task: {e}")}).to_string(),
    }
}

async fn execute_mark_task_done(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let task_id = match args.get("task_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "task_id is required"}).to_string(),
    };

    match crate::productivity::task_service::update_status(
        ctx.pool,
        task_id,
        ctx.user_id,
        TaskStatus::Done,
    )
    .await
    {
        Ok(task) => serde_json::json!({
            "success": true,
            "task": {
                "id": task.id,
                "title": task.title,
                "status": task.status.as_str(),
            }
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to mark task done: {e}")}).to_string()
        }
    }
}

async fn execute_mark_task_cancelled(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let task_id = match args.get("task_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "task_id is required"}).to_string(),
    };

    match crate::productivity::task_service::update_status(
        ctx.pool,
        task_id,
        ctx.user_id,
        TaskStatus::Cancelled,
    )
    .await
    {
        Ok(task) => serde_json::json!({
            "success": true,
            "task": {
                "id": task.id,
                "title": task.title,
                "status": task.status.as_str(),
            }
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to cancel task: {e}")}).to_string(),
    }
}

async fn execute_log_weight(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let weight_kg = match crate::ai::tool_loop::parse_body_weight_kg(args.get("weight_kg")) {
        Ok(w) => w,
        Err(msg) => {
            return serde_json::json!({"error": msg}).to_string();
        }
    };

    let recorded_at = parse_datetime_arg(args, "recorded_at", ctx.timezone);

    match crate::db::weight::insert(
        ctx.pool,
        crate::db::weight::NewWeightEntry {
            user_id: ctx.user_id,
            weight_kg,
            recorded_at,
        },
    )
    .await
    {
        Ok(entry) => serde_json::json!({
            "success": true,
            "weight_entry": {
                "id": entry.id,
                "weight_kg": entry.weight_kg,
                "recorded_at": entry.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
            }
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to log weight: {e}")}).to_string(),
    }
}

async fn execute_log_training(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let description = match args.get("description").and_then(|v| v.as_str()) {
        Some(d) if !d.trim().is_empty() => d.trim().to_string(),
        _ => {
            return serde_json::json!({"error": "description is required and must be non-empty"})
                .to_string();
        }
    };

    let recorded_at = parse_datetime_arg(args, "recorded_at", ctx.timezone);
    let activity_slug = args
        .get("activity")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase());

    match crate::db::training::try_insert(
        ctx.pool,
        crate::db::training::NewTrainingEntry {
            user_id: ctx.user_id,
            description,
            recorded_at,
            activity_slug,
            endurance: parse_endurance_arg(args.get("endurance")),
            strength_sets: parse_strength_sets_arg(ctx, args.get("strength_sets")).await,
        },
    )
    .await
    {
        Ok(crate::db::training::TrainingInsert::Created(entry)) => serde_json::json!({
            "success": true,
            "training_entry": training_entry_json(&entry),
        })
        .to_string(),
        Ok(crate::db::training::TrainingInsert::Duplicate(entry)) => serde_json::json!({
            "success": true,
            "duplicate": true,
            "message": "A training was already logged just now; reused that entry instead of creating another.",
            "training_entry": training_entry_json(&entry),
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to log training: {e}")}).to_string(),
    }
}

async fn execute_update_training(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let training_id = match args.get("training_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "training_id is required"}).to_string(),
    };

    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let recorded_at = parse_datetime_arg(args, "recorded_at", ctx.timezone);
    let activity_slug = args
        .get("activity")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase());
    let endurance = parse_endurance_arg(args.get("endurance"));
    let strength_sets = if args.get("strength_sets").is_some() {
        Some(parse_strength_sets_arg(ctx, args.get("strength_sets")).await)
    } else {
        None
    };

    if description.is_none()
        && recorded_at.is_none()
        && activity_slug.is_none()
        && endurance.is_none()
        && strength_sets.is_none()
    {
        return serde_json::json!({
            "error": "Provide at least one field to update"
        })
        .to_string();
    }

    match crate::db::training::update(
        ctx.pool,
        training_id,
        ctx.user_id,
        ctx.is_admin,
        crate::db::training::UpdateTraining {
            description,
            recorded_at,
            activity_slug,
            endurance,
            strength_sets,
        },
    )
    .await
    {
        Ok(Some(entry)) => serde_json::json!({
            "success": true,
            "training_entry": training_entry_json(&entry),
        })
        .to_string(),
        Ok(None) => serde_json::json!({
            "error": "Training not found, or you are not allowed to change it"
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to update training: {e}")}).to_string()
        }
    }
}

async fn execute_remove_training(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let training_id = match args.get("training_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "training_id is required"}).to_string(),
    };

    match crate::db::training::delete(ctx.pool, training_id, ctx.user_id, ctx.is_admin).await {
        Ok(true) => serde_json::json!({
            "success": true,
            "message": format!("Training #{training_id} deleted")
        })
        .to_string(),
        Ok(false) => serde_json::json!({
            "error": "Training not found, or you are not allowed to delete it"
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to delete training: {e}")}).to_string()
        }
    }
}

async fn execute_show_weight_chart(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let (bot, chat_id) = match (ctx.bot, ctx.chat_id) {
        (Some(bot), Some(chat_id)) => (bot, teloxide::types::ChatId(chat_id)),
        _ => {
            return serde_json::json!({
                "error": "Cannot send a weight chart outside an interactive Telegram chat. Ask the user to run /weightchart."
            })
            .to_string();
        }
    };

    let days_arg = args.get("days").and_then(|v| v.as_str()).unwrap_or("90");
    let days = match crate::bot::weight_graph::parse_days_arg(days_arg) {
        Ok(d) => d,
        Err(()) => {
            return serde_json::json!({
                "error": "Invalid days value. Use \"30\", \"90\", or \"all\"."
            })
            .to_string();
        }
    };

    let entries = match crate::db::weight::list_for_user(ctx.pool, ctx.user_id).await {
        Ok(e) => e,
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to fetch weight entries: {e}")})
                .to_string();
        }
    };

    let goal_kg = match crate::db::goals::list_active(ctx.pool, ctx.user_id).await {
        Ok(goals) => goals
            .into_iter()
            .find(|g| {
                crate::db::goals::GoalType::from_str(&g.goal_type)
                    == Some(crate::db::goals::GoalType::Weight)
            })
            .map(|g| g.target_value),
        Err(_) => None,
    };

    let user_lang = crate::i18n::Msg::new(ctx.lang);
    let chart = match crate::bot::weight_graph::build_weight_chart(
        &entries,
        ctx.timezone,
        days,
        goal_kg,
        &user_lang,
    ) {
        Ok(c) => c,
        Err(crate::bot::weight_graph::BuildWeightChartError::NoEntries) => {
            return serde_json::json!({
                "error": "No weight entries yet. Ask the user to log a weight first."
            })
            .to_string();
        }
        Err(crate::bot::weight_graph::BuildWeightChartError::NoDataInWindow) => {
            return serde_json::json!({
                "error": "No weight entries in that time window. Try days=\"all\"."
            })
            .to_string();
        }
        Err(crate::bot::weight_graph::BuildWeightChartError::Render(e)) => {
            return serde_json::json!({"error": format!("Failed to render chart: {e}")})
                .to_string();
        }
    };

    let mut req = bot
        .send_photo(
            chat_id,
            teloxide::types::InputFile::memory(chart.png).file_name("weight.png"),
        )
        .caption(chart.caption);
    if let Some(reply_to) = ctx.reply_to_message_id {
        use teloxide::sugar::request::RequestReplyExt;
        req = req.reply_to(reply_to);
    }
    match req.await {
        Ok(_) => serde_json::json!({
            "success": true,
            "sent": true,
            "window": chart.window_label,
            "points": chart.point_count,
            "start_kg": chart.start_kg,
            "end_kg": chart.end_kg,
            "delta_kg": chart.delta_kg,
            "goal_kg": chart.goal_kg,
            "note": "Chart image was sent to the chat. Briefly acknowledge it; do not invent another chart."
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to send chart photo: {e}")}).to_string(),
    }
}

fn meal_item_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "food_id": { "type": "integer", "description": "Catalog food id when known" },
            "name": { "type": "string", "description": "Catalog name to resolve if food_id is missing" },
            "quantity": { "type": "number" },
            "unit": { "type": "string", "enum": ["g", "ml", "serving", "piece"] },
            "description": { "type": "string" },
            "kcal": { "type": "number", "description": "Required for estimated items" },
            "protein_g": { "type": "number" },
            "carbs_g": { "type": "number" },
            "fat_g": { "type": "number" },
            "source": { "type": "string", "enum": ["catalog", "estimated", "photo"] }
        },
        "required": ["quantity", "unit"]
    })
}

fn food_json(food: &crate::db::foods::Food) -> serde_json::Value {
    serde_json::json!({
        "id": food.id,
        "name": food.name,
        "brand": food.brand,
        "barcode": food.barcode,
        "notes": food.notes,
        "kcal_per_100": food.kcal_per_100,
        "protein_g_per_100": food.protein_g_per_100,
        "carbs_g_per_100": food.carbs_g_per_100,
        "fat_g_per_100": food.fat_g_per_100,
        "serving_qty": food.serving_qty,
        "serving_unit": food.serving_unit.as_str(),
        "serving_label": food.serving_label,
        "source": food.source.as_str(),
    })
}

fn macros_json(m: &crate::db::foods::Macros) -> serde_json::Value {
    serde_json::json!({
        "kcal": m.kcal,
        "protein_g": m.protein_g,
        "carbs_g": m.carbs_g,
        "fat_g": m.fat_g,
    })
}

fn meal_json(meal: &crate::db::meals::Meal) -> serde_json::Value {
    serde_json::json!({
        "id": meal.id,
        "kind": meal.kind.as_str(),
        "label": meal.label,
        "eaten_at": meal.eaten_at.to_rfc3339(),
        "notes": meal.notes,
        "totals": macros_json(&meal.totals()),
        "items": meal.items.iter().map(|item| serde_json::json!({
            "id": item.id,
            "food_id": item.food_id,
            "description": item.description,
            "quantity": item.quantity,
            "unit": item.unit.as_str(),
            "source": item.source.as_str(),
            "kcal": item.macros.kcal,
            "protein_g": item.macros.protein_g,
            "carbs_g": item.macros.carbs_g,
            "fat_g": item.macros.fat_g,
        })).collect::<Vec<_>>(),
    })
}

fn day_json(day: &crate::db::meals::DayNutrition) -> serde_json::Value {
    serde_json::json!({
        "date": day.date.format("%Y-%m-%d").to_string(),
        "timezone": day.timezone,
        "totals": macros_json(&day.totals),
        "meals": day.meals.iter().map(meal_json).collect::<Vec<_>>(),
    })
}

fn parse_nutrition_date(
    args: &serde_json::Value,
    timezone: &str,
) -> Result<chrono::NaiveDate, String> {
    let raw = args.get("date").and_then(|v| v.as_str()).unwrap_or("");
    crate::bot::nutrition_table::parse_date_arg(raw, timezone, chrono::Utc::now())
        .map_err(|_| "Invalid date. Use today, yesterday, or YYYY-MM-DD.".to_string())
}

fn parse_unit(
    args: &serde_json::Value,
    key: &str,
    default: &str,
) -> Result<crate::db::foods::QuantityUnit, String> {
    let raw = args.get(key).and_then(|v| v.as_str()).unwrap_or(default);
    crate::db::foods::QuantityUnit::parse(raw).map_err(|e| e.to_string())
}

fn opt_string(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn parse_food_source(args: &serde_json::Value) -> crate::db::foods::FoodSource {
    match args
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("manual")
    {
        "photo" => crate::db::foods::FoodSource::Photo,
        "api" => crate::db::foods::FoodSource::Api,
        _ => crate::db::foods::FoodSource::Manual,
    }
}

fn nutrient_per_100(
    args: &serde_json::Value,
    per_100_key: &str,
    serving_key: &str,
    serving_qty: f64,
) -> Result<f64, String> {
    if let Some(v) = args.get(per_100_key).and_then(|x| x.as_f64()) {
        return Ok(v);
    }
    let Some(v) = args.get(serving_key).and_then(|x| x.as_f64()) else {
        return Err(format!("{per_100_key} or {serving_key} is required"));
    };
    crate::db::foods::per_100_from_serving(v, serving_qty).map_err(|e| e.to_string())
}

async fn maybe_embed_food(ctx: &ToolCtx<'_>, food: &crate::db::foods::Food) {
    crate::ai::food_selection::maybe_embed_food(ctx.pool, ctx.api_key, ctx.embedding_model, food)
        .await;
}

async fn maybe_embed_preference(
    ctx: &ToolCtx<'_>,
    pref: &crate::db::food_preferences::FoodPreference,
) {
    let Some(api_key) = ctx.api_key.filter(|k| !k.is_empty()) else {
        return;
    };
    if let Err(e) =
        crate::ai::food_selection::embed_preference(ctx.pool, api_key, ctx.embedding_model, pref)
            .await
    {
        tracing::warn!(
            preference_id = pref.id,
            "Failed to embed food preference: {e:#}"
        );
    }
}

async fn resolve_new_item(
    ctx: &ToolCtx<'_>,
    item: &serde_json::Value,
) -> Result<crate::db::meals::NewMealItem, String> {
    let quantity = item
        .get("quantity")
        .and_then(|v| v.as_f64())
        .filter(|q| *q > 0.0)
        .ok_or_else(|| "item quantity must be a positive number".to_string())?;
    let unit = parse_unit(item, "unit", "g")?;
    let description = opt_string(item, "description");
    let forced_estimated = item
        .get("source")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s == "estimated");
    let source = match item.get("source").and_then(|v| v.as_str()) {
        Some("photo") => crate::db::meals::ItemSource::Photo,
        Some("estimated") => crate::db::meals::ItemSource::Estimated,
        _ => crate::db::meals::ItemSource::Catalog,
    };

    let estimated_macros = match (
        item.get("kcal").and_then(|v| v.as_f64()),
        item.get("protein_g").and_then(|v| v.as_f64()),
        item.get("carbs_g").and_then(|v| v.as_f64()),
        item.get("fat_g").and_then(|v| v.as_f64()),
    ) {
        (Some(kcal), Some(protein_g), Some(carbs_g), Some(fat_g)) => {
            Some(crate::db::foods::Macros {
                kcal,
                protein_g,
                carbs_g,
                fat_g,
            })
        }
        _ => None,
    };

    if !forced_estimated {
        if let Some(food_id) = item.get("food_id").and_then(|v| v.as_i64()) {
            let food = crate::db::foods::get_for_user(ctx.pool, ctx.user_id, food_id)
                .await
                .map_err(|e| format!("Failed to load food: {e}"))?
                .ok_or_else(|| format!("food_id {food_id} not found"))?;
            return crate::db::meals::item_from_food(&food, quantity, unit, description)
                .map_err(|e| e.to_string());
        }
        if let Some(name) = opt_string(item, "name") {
            let hits = crate::ai::food_selection::resolve_food(
                ctx.pool,
                ctx.user_id,
                &name,
                ctx.api_key,
                ctx.embedding_model,
            )
            .await
            .map_err(|e| format!("Failed to resolve food: {e}"))?;
            if hits.len() == 1 {
                return crate::db::meals::item_from_food(
                    &hits[0].food,
                    quantity,
                    unit,
                    description,
                )
                .map_err(|e| e.to_string());
            }
            if hits.len() > 1 {
                let candidates: Vec<_> = hits.iter().map(|h| food_json(&h.food)).collect();
                return Err(serde_json::json!({
                    "error": format!("Ambiguous food name '{name}'. Pass food_id."),
                    "candidates": candidates
                })
                .to_string());
            }
        }
    }

    let macros = estimated_macros.ok_or_else(|| {
        "Unknown food: pass food_id/name from the catalog, or estimated kcal/protein_g/carbs_g/fat_g".to_string()
    })?;
    let desc = description
        .or_else(|| opt_string(item, "name"))
        .ok_or_else(|| "description or name is required for estimated items".to_string())?;
    // Invented macros are never catalog values; keep photo when the caller said so.
    let source = match source {
        crate::db::meals::ItemSource::Catalog => crate::db::meals::ItemSource::Estimated,
        other => other,
    };
    crate::db::meals::estimated_item(desc, quantity, unit, macros, source)
        .map_err(|e| e.to_string())
}

async fn execute_search_foods(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let query = opt_string(args, "query").unwrap_or_default();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 40) as usize;

    let result = if query.is_empty() {
        crate::db::foods::list_for_user(ctx.pool, ctx.user_id).await
    } else {
        crate::ai::food_selection::resolve_food(
            ctx.pool,
            ctx.user_id,
            &query,
            ctx.api_key,
            ctx.embedding_model,
        )
        .await
        .map(|hits| hits.into_iter().map(|h| h.food).collect())
    };

    match result {
        Ok(foods) => {
            let foods: Vec<_> = foods
                .into_iter()
                .take(limit)
                .map(|f| food_json(&f))
                .collect();
            serde_json::json!({ "foods": foods, "count": foods.len() }).to_string()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to search foods: {e}")}).to_string(),
    }
}

async fn execute_register_food(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let name = match opt_string(args, "name") {
        Some(n) => n,
        None => return serde_json::json!({"error": "name is required"}).to_string(),
    };
    let serving_qty = args
        .get("serving_qty")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let serving_unit = match parse_unit(args, "serving_unit", "g") {
        Ok(u) => u,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    let kcal_per_100 = match nutrient_per_100(args, "kcal_per_100", "kcal", serving_qty) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    let protein_g_per_100 =
        match nutrient_per_100(args, "protein_g_per_100", "protein_g", serving_qty) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };
    let carbs_g_per_100 = match nutrient_per_100(args, "carbs_g_per_100", "carbs_g", serving_qty) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    let fat_g_per_100 = match nutrient_per_100(args, "fat_g_per_100", "fat_g", serving_qty) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };

    match crate::db::foods::insert(
        ctx.pool,
        crate::db::foods::NewFood {
            user_id: ctx.user_id,
            name,
            brand: opt_string(args, "brand"),
            barcode: opt_string(args, "barcode"),
            notes: opt_string(args, "notes"),
            kcal_per_100,
            protein_g_per_100,
            carbs_g_per_100,
            fat_g_per_100,
            serving_qty,
            serving_unit,
            serving_label: opt_string(args, "serving_label"),
            source: parse_food_source(args),
        },
    )
    .await
    {
        Ok(food) => {
            maybe_embed_food(ctx, &food).await;
            serde_json::json!({"success": true, "food": food_json(&food)}).to_string()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to register food: {e}")}).to_string(),
    }
}

async fn execute_update_food(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let food_id = match args.get("food_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "food_id is required"}).to_string(),
    };
    let serving_unit = match args.get("serving_unit").and_then(|v| v.as_str()) {
        Some(s) => match crate::db::foods::QuantityUnit::parse(s) {
            Ok(u) => Some(u),
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        },
        None => None,
    };
    let patch = crate::db::foods::FoodUpdate {
        name: opt_string(args, "name"),
        brand: args.get("brand").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        barcode: args.get("barcode").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        notes: args.get("notes").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        kcal_per_100: args.get("kcal_per_100").and_then(|v| v.as_f64()),
        protein_g_per_100: args.get("protein_g_per_100").and_then(|v| v.as_f64()),
        carbs_g_per_100: args.get("carbs_g_per_100").and_then(|v| v.as_f64()),
        fat_g_per_100: args.get("fat_g_per_100").and_then(|v| v.as_f64()),
        serving_qty: args.get("serving_qty").and_then(|v| v.as_f64()),
        serving_unit,
        serving_label: args.get("serving_label").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        source: None,
    };
    match crate::db::foods::update(ctx.pool, ctx.user_id, food_id, patch).await {
        Ok(food) => {
            maybe_embed_food(ctx, &food).await;
            serde_json::json!({"success": true, "food": food_json(&food)}).to_string()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to update food: {e}")}).to_string(),
    }
}

fn fold_meal_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.to_lowercase().chars() {
        match c {
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'à' | 'á' | 'â' | 'ä' | 'ã' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace()
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn same_item_names(incoming: &[String], existing: &[String]) -> bool {
    if incoming.is_empty() || incoming.len() != existing.len() {
        return false;
    }
    let mut left: Vec<String> = incoming.iter().map(|s| fold_meal_name(s)).collect();
    let mut right: Vec<String> = existing.iter().map(|s| fold_meal_name(s)).collect();
    if left.iter().any(|s| s.is_empty()) || right.iter().any(|s| s.is_empty()) {
        return false;
    }
    left.sort();
    right.sort();
    left == right
}

/// Catalog rows match on `food_id` (stored text is often `embed_text`, not the
/// name the model sent). Estimated rows match on folded descriptions.
fn same_resolved_items(
    incoming: &[crate::db::meals::NewMealItem],
    existing: &[crate::db::meals::MealItem],
) -> bool {
    if incoming.is_empty() || incoming.len() != existing.len() {
        return false;
    }
    let mut used = vec![false; existing.len()];
    for item in incoming {
        let Some(idx) = existing.iter().enumerate().position(|(i, stored)| {
            if used[i] {
                return false;
            }
            match (item.food_id, stored.food_id) {
                (Some(left), Some(right)) => left == right,
                _ => {
                    let left = fold_meal_name(&item.description);
                    let right = fold_meal_name(&stored.description);
                    !left.is_empty() && left == right
                }
            }
        }) else {
            return false;
        };
        used[idx] = true;
    }
    true
}

fn already_logged_meal_response(meal: &crate::db::meals::Meal, date: chrono::NaiveDate) -> String {
    let date_s = date.format("%Y-%m-%d").to_string();
    let items: Vec<_> = meal
        .items
        .iter()
        .map(|item| {
            serde_json::json!({
                "food_id": item.food_id,
                "description": item.description,
                "quantity": item.quantity,
                "unit": item.unit.as_str(),
            })
        })
        .collect();
    serde_json::json!({
        "error": "already logged",
        "meal_id": meal.id,
        "date": date_s,
        "items": items,
        "hint": format!(
            "Those items are already on the diary for {date_s}. Do not log them again. Answer from this meal."
        ),
    })
    .to_string()
}

async fn execute_log_meal(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let kind = match args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(crate::db::meals::MealKind::parse)
    {
        Some(Ok(k)) => k,
        _ => {
            return serde_json::json!({"error": "kind must be breakfast, lunch, dinner, snack, or other"})
                .to_string();
        }
    };
    let Some(items_val) = args.get("items").and_then(|v| v.as_array()) else {
        return serde_json::json!({"error": "items is required"}).to_string();
    };
    if items_val.is_empty() {
        return serde_json::json!({"error": "items must not be empty"}).to_string();
    }

    let eaten_at =
        parse_datetime_arg(args, "eaten_at", ctx.timezone).unwrap_or_else(chrono::Utc::now);
    let date = eaten_at
        .with_timezone(&ctx.timezone.parse().unwrap_or(chrono_tz::UTC))
        .date_naive();

    let mut items = Vec::new();
    for item in items_val {
        match resolve_new_item(ctx, item).await {
            Ok(i) => items.push(i),
            Err(e) => {
                if e.trim_start().starts_with('{') {
                    return e;
                }
                return serde_json::json!({"error": e}).to_string();
            }
        }
    }

    if let Ok(day) = crate::db::meals::day_for_user(ctx.pool, ctx.user_id, ctx.timezone, date).await
        && let Some(meal) = day
            .meals
            .iter()
            .find(|meal| same_resolved_items(&items, &meal.items))
    {
        return already_logged_meal_response(meal, date);
    }

    match crate::db::meals::insert(
        ctx.pool,
        crate::db::meals::NewMeal {
            user_id: ctx.user_id,
            kind,
            label: opt_string(args, "label"),
            eaten_at,
            notes: opt_string(args, "notes"),
            items,
        },
    )
    .await
    {
        Ok(meal) => {
            let date = meal
                .eaten_at
                .with_timezone(&ctx.timezone.parse().unwrap_or(chrono_tz::UTC))
                .date_naive();
            let day = crate::db::meals::day_for_user(ctx.pool, ctx.user_id, ctx.timezone, date)
                .await
                .ok();
            serde_json::json!({
                "success": true,
                "meal": meal_json(&meal),
                "day_totals": day.as_ref().map(|d| macros_json(&d.totals)),
            })
            .to_string()
        }
        Err(e) => serde_json::json!({"error": format!("Failed to log meal: {e}")}).to_string(),
    }
}

async fn execute_get_day_nutrition(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let date = match parse_nutrition_date(args, ctx.timezone) {
        Ok(d) => d,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    match crate::db::meals::day_for_user(ctx.pool, ctx.user_id, ctx.timezone, date).await {
        Ok(day) => day_json(&day).to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to load nutrition: {e}")}).to_string()
        }
    }
}

async fn execute_show_nutrition_table(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let (bot, chat_id) = match (ctx.bot, ctx.chat_id) {
        (Some(bot), Some(chat_id)) => (bot, teloxide::types::ChatId(chat_id)),
        _ => {
            return serde_json::json!({
                "error": "Cannot send a nutrition table outside an interactive Telegram chat. Ask the user to run /nutrition."
            })
            .to_string();
        }
    };
    if chat_id.0 < 0 {
        return serde_json::json!({
            "error": "Nutrition is private. Ask the user to open a 1:1 chat and run /nutrition."
        })
        .to_string();
    }
    let date = match parse_nutrition_date(args, ctx.timezone) {
        Ok(d) => d,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    let day = match crate::db::meals::day_for_user(ctx.pool, ctx.user_id, ctx.timezone, date).await
    {
        Ok(d) => d,
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to load nutrition: {e}")})
                .to_string();
        }
    };
    let user_lang = crate::i18n::Msg::new(ctx.lang);
    let table = match crate::bot::nutrition_table::build_nutrition_table(
        &day,
        ctx.timezone,
        &user_lang,
    ) {
        Ok(t) => t,
        Err(crate::bot::nutrition_table::BuildNutritionTableError::Empty) => {
            return serde_json::json!({
                "error": format!("No meals logged for {}. Ask the user to log a meal first.", date)
            })
            .to_string();
        }
        Err(crate::bot::nutrition_table::BuildNutritionTableError::Render(e)) => {
            return serde_json::json!({"error": format!("Failed to render nutrition table: {e}")})
                .to_string();
        }
    };

    let mut req = bot
        .send_photo(
            chat_id,
            teloxide::types::InputFile::memory(table.png).file_name("nutrition.png"),
        )
        .caption(table.caption);
    if let Some(reply_to) = ctx.reply_to_message_id {
        use teloxide::sugar::request::RequestReplyExt;
        req = req.reply_to(reply_to);
    }
    match req.await {
        Ok(_) => serde_json::json!({
            "success": true,
            "sent": true,
            "date": table.date.format("%Y-%m-%d").to_string(),
            "meal_count": table.meal_count,
            "totals": macros_json(&table.totals),
            "note": "Nutrition table image was sent to the chat. Briefly acknowledge it; do not invent another table."
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to send nutrition table: {e}")}).to_string(),
    }
}

async fn execute_update_meal(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let meal_id = match args.get("meal_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "meal_id is required"}).to_string(),
    };
    let kind = match args.get("kind").and_then(|v| v.as_str()) {
        Some(s) => match crate::db::meals::MealKind::parse(s) {
            Ok(k) => Some(k),
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        },
        None => None,
    };
    let items = if let Some(arr) = args.get("items").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for item in arr {
            match resolve_new_item(ctx, item).await {
                Ok(i) => out.push(i),
                Err(e) => {
                    if e.trim_start().starts_with('{') {
                        return e;
                    }
                    return serde_json::json!({"error": e}).to_string();
                }
            }
        }
        Some(out)
    } else {
        None
    };
    let patch = crate::db::meals::MealUpdate {
        kind,
        label: args.get("label").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        eaten_at: parse_datetime_arg(args, "eaten_at", ctx.timezone),
        notes: args.get("notes").and_then(|v| v.as_str()).map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        items,
    };
    match crate::db::meals::update(ctx.pool, ctx.user_id, meal_id, patch).await {
        Ok(meal) => serde_json::json!({"success": true, "meal": meal_json(&meal)}).to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to update meal: {e}")}).to_string(),
    }
}

async fn execute_remove_meal(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let meal_id = match args.get("meal_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "meal_id is required"}).to_string(),
    };
    match crate::db::meals::delete(ctx.pool, ctx.user_id, meal_id).await {
        Ok(true) => serde_json::json!({
            "success": true,
            "message": format!("Meal #{meal_id} deleted")
        })
        .to_string(),
        Ok(false) => serde_json::json!({"error": "Meal not found, or not yours"}).to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to delete meal: {e}")}).to_string(),
    }
}

fn preference_json(pref: &crate::db::food_preferences::FoodPreference) -> serde_json::Value {
    serde_json::json!({
        "id": pref.id,
        "kind": pref.kind.as_str(),
        "text": pref.text,
        "food_id": pref.food_id,
    })
}

fn proposed_item_json(item: &crate::ai::food_propose::ProposedItem) -> serde_json::Value {
    serde_json::json!({
        "food_id": item.food_id,
        "description": item.description,
        "quantity": item.quantity,
        "unit": item.unit.as_str(),
        "macros": item.macros.as_ref().map(macros_json),
        "estimated": item.estimated,
    })
}

fn meal_option_json(option: &crate::ai::food_propose::MealOption) -> serde_json::Value {
    serde_json::json!({
        "title": option.title,
        "items": option.items.iter().map(proposed_item_json).collect::<Vec<_>>(),
        "macros": option.macros.as_ref().map(macros_json),
        "day_if_eaten": option.day_if_eaten.as_ref().map(macros_json),
        "why": option.why,
        "estimated": option.estimated,
    })
}

fn nutrition_private_error() -> String {
    serde_json::json!({
        "error": "Nutrition is private. Ask the user to open a 1:1 chat."
    })
    .to_string()
}

async fn execute_propose_meals(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    if ctx.chat_id.is_some_and(|id| id < 0) {
        return nutrition_private_error();
    }
    let Some(envie) = opt_string(args, "envie") else {
        return serde_json::json!({"error": "envie is required"}).to_string();
    };
    let kind = match args.get("kind").and_then(|v| v.as_str()) {
        Some(s) => match crate::db::meals::MealKind::parse(s) {
            Ok(k) => Some(k),
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        },
        None => None,
    };
    let minutes = args
        .get("minutes")
        .and_then(|v| v.as_i64())
        .filter(|m| *m > 0)
        .map(|m| m.min(24 * 60) as u32);

    let tz: chrono_tz::Tz = ctx.timezone.parse().unwrap_or(chrono_tz::UTC);
    let today = chrono::Utc::now().with_timezone(&tz).date_naive();
    let day = match crate::db::meals::day_for_user(ctx.pool, ctx.user_id, ctx.timezone, today).await
    {
        Ok(d) => d,
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to load today's diary: {e}")})
                .to_string();
        }
    };

    let catalog = match crate::db::foods::list_for_user(ctx.pool, ctx.user_id).await {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to load catalog: {e}")})
                .to_string();
        }
    };
    let prefs = match crate::db::food_preferences::list_for_user(ctx.pool, ctx.user_id).await {
        Ok(p) => p,
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to load preferences: {e}")})
                .to_string();
        }
    };

    let food_embeddings: std::collections::HashMap<i64, Vec<f32>> =
        match crate::db::foods::list_embeddings(ctx.pool, ctx.user_id).await {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    crate::ai::food_selection::parse_embedding_json(&row.embedding)
                        .map(|v| (row.food_id, v))
                })
                .collect(),
            Err(_) => std::collections::HashMap::new(),
        };
    let pref_embeddings: std::collections::HashMap<i64, Vec<f32>> =
        match crate::db::food_preferences::list_embeddings(ctx.pool, ctx.user_id).await {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    crate::ai::food_selection::parse_embedding_json(&row.embedding)
                        .map(|v| (row.preference_id, v))
                })
                .collect(),
            Err(_) => std::collections::HashMap::new(),
        };

    let envie_emb = match ctx.api_key.filter(|k| !k.is_empty()) {
        Some(api_key) => match crate::embeddings::embed(api_key, ctx.embedding_model, &envie).await
        {
            Ok((v, _usage)) => Some(v),
            Err(e) => {
                tracing::warn!("envie embed failed: {e:#}");
                None
            }
        },
        None => None,
    };

    let ranked = crate::ai::food_selection::rank_catalog(
        &envie,
        envie_emb.as_deref(),
        &catalog,
        &food_embeddings,
        &prefs,
        &pref_embeddings,
    );

    let training_entries = crate::db::training::list_for_user(ctx.pool, ctx.user_id)
        .await
        .unwrap_or_default();
    let (day_start, day_end) =
        crate::ai::food_propose::today_bounds(ctx.timezone, chrono::Utc::now())
            .unwrap_or((chrono::Utc::now(), chrono::Utc::now()));
    let today_training =
        crate::ai::food_propose::training_today(&training_entries, day_start, day_end);
    let owned: Vec<crate::db::training::TrainingEntry> =
        today_training.into_iter().cloned().collect();
    let bias = crate::ai::food_propose::classify_training(&owned);

    let mut memories = Vec::new();
    if ctx.rag_enabled
        && let Some(emb) = envie_emb.as_deref()
    {
        match crate::db::chunks::search(ctx.pool, ctx.user_id, emb, 3).await {
            Ok(hits) => {
                memories = hits
                    .into_iter()
                    .map(|h| {
                        serde_json::json!({
                            "created_at": h.created_at,
                            "content": h.content,
                        })
                    })
                    .collect();
            }
            Err(e) => tracing::warn!("envie RAG search failed: {e:#}"),
        }
    }

    let options =
        crate::ai::food_propose::build_options(&envie, &ranked, &day.totals, bias, minutes, kind);

    serde_json::json!({
        "envie": envie,
        "kind": kind.map(|k| k.as_str()),
        "minutes": minutes,
        "training_bias": bias.as_str(),
        "training_today": owned.iter().map(|e| e.activity_slug.clone()).collect::<Vec<_>>(),
        "day": {
            "date": day.date.format("%Y-%m-%d").to_string(),
            "totals": macros_json(&day.totals),
            "meals_logged": day.meals.len(),
        },
        "preferences": prefs.iter().map(preference_json).collect::<Vec<_>>(),
        "memories": memories,
        "options": options.iter().map(meal_option_json).collect::<Vec<_>>(),
        "note": "Do not auto-log. Each option includes macros and day_if_eaten from the nutrition diary — cite those numbers. If the user picks an option, call log_meal with those items. Estimated options have no grams — say so (~). Never treat session calories as TDEE."
    })
    .to_string()
}

async fn execute_remember_food_preference(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    if ctx.chat_id.is_some_and(|id| id < 0) {
        return nutrition_private_error();
    }
    let kind = match args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(crate::db::food_preferences::PreferenceKind::parse)
    {
        Some(Ok(k)) => k,
        _ => {
            return serde_json::json!({
                "error": "kind must be allergy, dislike, like, or habit"
            })
            .to_string();
        }
    };
    let Some(text) = opt_string(args, "text") else {
        return serde_json::json!({"error": "text is required"}).to_string();
    };
    let food_id = args.get("food_id").and_then(|v| v.as_i64());
    if let Some(id) = food_id {
        match crate::db::foods::get_for_user(ctx.pool, ctx.user_id, id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return serde_json::json!({"error": format!("food_id {id} not found")}).to_string();
            }
            Err(e) => {
                return serde_json::json!({"error": format!("Failed to load food: {e}")})
                    .to_string();
            }
        }
    }

    match crate::db::food_preferences::insert(
        ctx.pool,
        crate::db::food_preferences::NewFoodPreference {
            user_id: ctx.user_id,
            kind,
            text,
            food_id,
        },
    )
    .await
    {
        Ok(pref) => {
            maybe_embed_preference(ctx, &pref).await;
            serde_json::json!({
                "success": true,
                "preference": preference_json(&pref)
            })
            .to_string()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to save preference: {e}")}).to_string()
        }
    }
}

async fn execute_send_coach_image(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(kind, "victory" | "sarcasm" | "edit") {
        return serde_json::json!({
            "error": "kind must be victory, sarcasm, or edit"
        })
        .to_string();
    }
    let scene = args
        .get("scene")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(scene) = scene else {
        return serde_json::json!({"error": "scene is required"}).to_string();
    };
    let photo_id = args.get("photo_id").and_then(|v| v.as_i64());
    let mut stored_jpeg: Option<Vec<u8>> = None;
    if let Some(id) = photo_id {
        match crate::db::photo::jpeg_for_user(ctx.pool, id, ctx.user_id).await {
            Ok(Some(bytes)) => stored_jpeg = Some(bytes),
            Ok(None) => {
                return serde_json::json!({
                    "error": "photo_id not found or not owned by this user"
                })
                .to_string();
            }
            Err(e) => {
                return serde_json::json!({"error": format!("Failed to load stored photo: {e}")})
                    .to_string();
            }
        }
    }

    let use_user_photo = args
        .get("use_user_photo")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || (kind == "edit" && stored_jpeg.is_none());

    match super::image::quota_today(
        ctx.pool,
        ctx.user_id,
        ctx.image_model,
        ctx.image_max_per_user_day,
    )
    .await
    {
        Ok(q) if q.remaining_today == 0 => {
            let mut body = q.to_json();
            body["error"] = serde_json::json!("quota_exceeded");
            body["quota_exceeded"] = serde_json::json!(true);
            body["note"] = serde_json::json!(
                "Daily image quota reached. Stay on text; do not retry this turn."
            );
            return body.to_string();
        }
        Ok(_) => {}
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to check image quota: {e}")})
                .to_string();
        }
    }

    if use_user_photo && ctx.user_photo.is_none() && stored_jpeg.is_none() {
        return serde_json::json!({
            "error": "No user photo on this turn and no photo_id. Set use_user_photo=false, pass photo_id from search_photos, or wait for a photo."
        })
        .to_string();
    }

    let (bot, chat_id) = match (ctx.bot, ctx.chat_id) {
        (Some(bot), Some(chat_id)) => (bot, teloxide::types::ChatId(chat_id)),
        _ => {
            return serde_json::json!({
                "error": "Cannot send a coach image outside an interactive Telegram chat."
            })
            .to_string();
        }
    };

    let Some(api_key) = ctx.api_key else {
        return serde_json::json!({"error": "Image generation is not configured."}).to_string();
    };

    let prompt = format!("Kind: {kind}. English visual scene (do not invent numbers): {scene}");
    let source = if let Some(ref jpeg) = stored_jpeg {
        Some((jpeg.as_slice(), crate::media::JPEG_MIME))
    } else {
        ctx.user_photo
            .filter(|_| use_user_photo)
            .map(|img| (img.bytes, img.mime_type))
    };

    let image = match super::image::generate_coach_image(
        api_key,
        ctx.image_model,
        &prompt,
        source,
        Some((ctx.pool, ctx.bot_token, ctx.user_id)),
    )
    .await
    {
        Ok(img) => img,
        Err(e) => {
            return serde_json::json!({"error": format!("Image generation failed: {e}")})
                .to_string();
        }
    };

    super::image::log_image_usage(
        ctx.pool,
        ctx.user_id,
        ctx.image_model,
        &image.usage,
        ctx.request_id,
    )
    .await;

    let caption = args
        .get("caption")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(400).collect::<String>())
        .unwrap_or_else(|| match ctx.lang {
            crate::i18n::Lang::Fr => "Affiche officielle.".into(),
            crate::i18n::Lang::En => "Official poster.".into(),
        });

    let filename = if image.mime_type.contains("png") {
        "coach.png"
    } else {
        "coach.jpg"
    };
    let mut req = bot
        .send_photo(
            chat_id,
            teloxide::types::InputFile::memory(image.bytes).file_name(filename),
        )
        .caption(caption);
    if let Some(reply_to) = ctx.reply_to_message_id {
        use teloxide::sugar::request::RequestReplyExt;
        req = req.reply_to(reply_to);
    }
    match req.await {
        Ok(_) => {
            let quota = super::image::quota_today(
                ctx.pool,
                ctx.user_id,
                ctx.image_model,
                ctx.image_max_per_user_day,
            )
            .await
            .ok();
            let mut body = serde_json::json!({
                "success": true,
                "sent": true,
                "kind": kind,
                "note": "Image was sent to the chat. Briefly acknowledge it; do not generate another this turn."
            });
            if let Some(q) = quota {
                body["max_per_day"] = serde_json::json!(q.max_per_day);
                body["used_today"] = serde_json::json!(q.used_today);
                body["remaining_today"] = serde_json::json!(q.remaining_today);
            }
            body.to_string()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to send coach image: {e}")}).to_string()
        }
    }
}

async fn execute_get_image_quota(ctx: &ToolCtx<'_>) -> String {
    match super::image::quota_today(
        ctx.pool,
        ctx.user_id,
        ctx.image_model,
        ctx.image_max_per_user_day,
    )
    .await
    {
        Ok(q) => {
            let mut body = q.to_json();
            body["enabled"] = serde_json::json!(ctx.api_key.is_some() && q.max_per_day > 0);
            body["note"] = serde_json::json!(
                "UTC calendar day. Tell the user remaining and max if they asked. Do not generate an image just to check quota."
            );
            body.to_string()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to read image quota: {e}")}).to_string()
        }
    }
}

async fn execute_search_photos(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(5)
        .clamp(1, 10);
    let send = args.get("send").and_then(|v| v.as_bool()).unwrap_or(false);
    let photo_id = args.get("photo_id").and_then(|v| v.as_i64());

    let owned_query_emb;
    let embedding: &[f32] = if let Some(q) = query {
        let Some(api_key) = ctx.api_key else {
            return serde_json::json!({"error": "Image search needs a Gemini token."}).to_string();
        };
        match crate::embeddings::embed(api_key, ctx.embedding_model, q).await {
            Ok((v, usage)) => {
                crate::ai::log::persist_embed(
                    ctx.pool,
                    ctx.user_id,
                    ctx.embedding_model,
                    usage,
                    crate::db::ai_usage::KIND_EMBED_SEARCH_PHOTOS,
                    ctx.request_id,
                )
                .await;
                owned_query_emb = v;
                &owned_query_emb
            }
            Err(e) => {
                return serde_json::json!({"error": format!("Failed to embed search query: {e}")})
                    .to_string();
            }
        }
    } else if let Some(emb) = ctx.prompt_embedding {
        emb
    } else {
        return serde_json::json!({
            "error": "Provide query or run this from an interactive /ask turn."
        })
        .to_string();
    };

    let scope = crate::db::photo::PhotoSearchScope::from_chat_id(ctx.chat_id);
    let hits =
        match crate::db::photo::search(ctx.pool, ctx.user_id, embedding, scope, limit, None, false)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                return serde_json::json!({"error": format!("Photo search failed: {e}")})
                    .to_string();
            }
        };

    let photos: Vec<serde_json::Value> = hits
        .iter()
        .map(|h| {
            serde_json::json!({
                "photo_id": h.photo_id,
                "taken_at": h.taken_at,
                "caption": h.caption,
                "distance": h.distance,
                "training_counted": h.training_counted,
            })
        })
        .collect();

    let send_id = photo_id.or_else(|| hits.first().map(|h| h.photo_id));
    let mut sent = false;
    if send {
        let Some(id) = send_id else {
            return serde_json::json!({
                "photos": photos,
                "note": "No stored photos matched."
            })
            .to_string();
        };
        let (bot, chat_id) = match (ctx.bot, ctx.chat_id) {
            (Some(bot), Some(chat_id)) => (bot, teloxide::types::ChatId(chat_id)),
            _ => {
                return serde_json::json!({
                    "photos": photos,
                    "error": "Cannot send a photo outside an interactive Telegram chat."
                })
                .to_string();
            }
        };
        match crate::db::photo::jpeg_for_user(ctx.pool, id, ctx.user_id).await {
            Ok(Some(bytes)) => {
                let mut req = bot.send_photo(
                    chat_id,
                    teloxide::types::InputFile::memory(bytes).file_name("photo.jpg"),
                );
                if let Some(hit) = hits.iter().find(|h| h.photo_id == id) {
                    let cap = format!(
                        "{}{}",
                        hit.taken_at,
                        hit.caption
                            .as_deref()
                            .map(|c| format!(" — {c}"))
                            .unwrap_or_default()
                    );
                    req = req.caption(cap);
                }
                if let Some(reply_to) = ctx.reply_to_message_id {
                    use teloxide::sugar::request::RequestReplyExt;
                    req = req.reply_to(reply_to);
                }
                match req.await {
                    Ok(_) => sent = true,
                    Err(e) => {
                        return serde_json::json!({
                            "photos": photos,
                            "error": format!("Failed to send photo: {e}")
                        })
                        .to_string();
                    }
                }
            }
            Ok(None) => {
                return serde_json::json!({
                    "photos": photos,
                    "error": "photo_id not found or not owned by this user"
                })
                .to_string();
            }
            Err(e) => {
                return serde_json::json!({"error": format!("Failed to load photo: {e}")})
                    .to_string();
            }
        }
    }

    serde_json::json!({
        "photos": photos,
        "sent": sent,
        "note": "Dates are UTC (Telegram message time). Use photo_id with send=true or send_coach_image. Screenshots and labels only — do not use this to track physique or ask for a body photo. Do not invent what is in a picture you have not seen."
    })
    .to_string()
}

async fn execute_set_nudge_settings(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let enabled = args.get("enabled").and_then(|v| v.as_bool());
    let inactivity_days = args
        .get("inactivity_days")
        .and_then(|v| v.as_i64())
        .filter(|d| *d >= 1 && *d <= 365);

    let user = match crate::db::users::find_by_id(ctx.pool, ctx.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return serde_json::json!({"error": "User not found"}).to_string(),
        Err(e) => {
            return serde_json::json!({"error": format!("Failed to lookup user: {e}")}).to_string();
        }
    };

    match crate::db::users::set_nudge_settings(
        ctx.pool,
        user.telegram_user_id,
        enabled,
        inactivity_days,
    )
    .await
    {
        Ok(true) => serde_json::json!({
            "success": true,
            "enabled": enabled.unwrap_or(user.nudges_enabled),
            "inactivity_days": inactivity_days.unwrap_or(user.nudge_inactivity_days),
        })
        .to_string(),
        Ok(false) => serde_json::json!({"error": "No changes made"}).to_string(),
        Err(e) => serde_json::json!({"error": format!("Failed to update nudge settings: {e}")})
            .to_string(),
    }
}

async fn execute_reminders(ctx: &ToolCtx<'_>) -> String {
    match crate::productivity::reminder_service::list_reminders(ctx.pool, ctx.user_id, None).await {
        Ok(reminders) => {
            let data: Vec<_> = reminders
                .iter()
                .map(|r| {
                    let message = match &r.target {
                        crate::productivity::domain::ReminderTarget::Custom { message } => {
                            Some(message.clone())
                        }
                        crate::productivity::domain::ReminderTarget::Task { task_id } => {
                            Some(format!("Task reminder #{task_id}"))
                        }
                    };
                    serde_json::json!({
                        "id": r.id,
                        "scheduled_at": r.scheduled_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                        "status": r.status.as_str(),
                        "message": message,
                    })
                })
                .collect();
            serde_json::to_string(&serde_json::json!({"reminders": data})).unwrap_or_default()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to fetch reminders: {e}")}).to_string()
        }
    }
}

async fn execute_update_reminder(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let reminder_id = match args.get("reminder_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "reminder_id is required"}).to_string(),
    };

    let scheduled_at = match parse_datetime_arg(args, "scheduled_at", ctx.timezone) {
        Some(d) => d,
        None => {
            return serde_json::json!({"error": "scheduled_at is required in RFC3339 format"})
                .to_string();
        }
    };

    match crate::productivity::reminder_service::update_reminder_time(
        ctx.pool,
        reminder_id,
        ctx.user_id,
        scheduled_at,
    )
    .await
    {
        Ok(true) => serde_json::json!({
            "success": true,
            "reminder": {
                "id": reminder_id,
                "scheduled_at": scheduled_at.format("%Y-%m-%d %H:%M UTC").to_string(),
            }
        })
        .to_string(),
        Ok(false) => {
            serde_json::json!({"error": "Reminder not found, not yours, or no longer pending"})
                .to_string()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to update reminder: {e}")}).to_string()
        }
    }
}

async fn execute_create_reminder(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let scheduled_at = match parse_datetime_arg(args, "scheduled_at", ctx.timezone) {
        Some(d) => d,
        None => {
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            return serde_json::json!({"error": format!("scheduled_at is required. Provide an RFC3339 timestamp. Current UTC: {now}")})
                .to_string();
        }
    };

    let message = args
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Reminder".to_string());

    let recurrence = args
        .get("recurrence")
        .and_then(|v| v.as_str())
        .and_then(crate::productivity::domain::ReminderRecurrence::from_str)
        .unwrap_or(crate::productivity::domain::ReminderRecurrence::None);

    let target = crate::productivity::domain::ReminderTarget::Custom { message };

    match crate::productivity::reminder_service::create_reminder(
        ctx.pool,
        ctx.user_id,
        target,
        scheduled_at,
        recurrence,
    )
    .await
    {
        Ok(reminder) => serde_json::json!({
            "success": true,
            "reminder": {
                "id": reminder.id,
                "scheduled_at": reminder.scheduled_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                "recurrence": reminder.recurrence.as_str(),
            }
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to create reminder: {e}")}).to_string()
        }
    }
}

async fn execute_cancel_reminder(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let reminder_id = match args.get("reminder_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => return serde_json::json!({"error": "reminder_id is required"}).to_string(),
    };

    match crate::productivity::reminder_service::cancel_reminder(ctx.pool, reminder_id, ctx.user_id)
        .await
    {
        Ok(true) => {
            serde_json::json!({"success": true, "message": "Reminder cancelled"}).to_string()
        }
        Ok(false) => serde_json::json!({"error": "Reminder not found or not yours"}).to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to cancel reminder: {e}")}).to_string()
        }
    }
}

async fn execute_create_proposal(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let title = match args.get("title").and_then(|v| v.as_str()) {
        Some(t) if !t.trim().is_empty() => t.to_string(),
        _ => {
            return serde_json::json!({"error": "title is required and must be non-empty"})
                .to_string();
        }
    };
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match crate::productivity::proposal_service::create_proposal(
        ctx.pool,
        ctx.user_id,
        title,
        description,
    )
    .await
    {
        Ok(proposal) => serde_json::json!({
            "success": true,
            "proposal": {
                "id": proposal.id,
                "title": proposal.title,
            }
        })
        .to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to create proposal: {e}")}).to_string()
        }
    }
}

async fn execute_list_proposals(ctx: &ToolCtx<'_>) -> String {
    match crate::productivity::proposal_service::list_proposals(ctx.pool, ctx.user_id, ctx.is_admin)
        .await
    {
        Ok(proposals) => {
            let data: Vec<_> = proposals
                .iter()
                .map(|p| {
                    let mut item = serde_json::json!({
                        "id": p.id,
                        "title": p.title,
                        "description": p.description,
                    });
                    if ctx.is_admin {
                        item["user_id"] = serde_json::json!(p.user_id);
                        if let Some(ref author) = p.author_username {
                            item["author"] = serde_json::json!(author);
                        }
                    }
                    item
                })
                .collect();
            serde_json::to_string(&serde_json::json!({"proposals": data})).unwrap_or_default()
        }
        Err(e) => {
            serde_json::json!({"error": format!("Failed to list proposals: {e}")}).to_string()
        }
    }
}

async fn execute_remove_proposal(ctx: &ToolCtx<'_>, args: &serde_json::Value) -> String {
    let Some(proposal_id) = args.get("proposal_id").and_then(|v| v.as_i64()) else {
        return serde_json::json!({"error": "proposal_id is required"}).to_string();
    };

    match crate::productivity::proposal_service::delete_proposal(
        ctx.pool,
        proposal_id,
        ctx.user_id,
        ctx.is_admin,
    )
    .await
    {
        Ok(true) => serde_json::json!({
            "success": true,
            "removed_proposal_id": proposal_id
        })
        .to_string(),
        Ok(false) => serde_json::json!({"error": "Proposal not found or not yours"}).to_string(),
        Err(e) => {
            serde_json::json!({"error": format!("Failed to remove proposal: {e}")}).to_string()
        }
    }
}

fn parse_datetime_arg(
    args: &serde_json::Value,
    key: &str,
    timezone: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = args.get(key)?.as_str()?;
    let tz: chrono_tz::Tz = timezone.parse().ok().unwrap_or(chrono_tz::UTC);
    crate::datetime::parse_flexible_datetime(s, &tz, chrono::Utc::now())
}

fn parse_endurance_arg(
    v: Option<&serde_json::Value>,
) -> Option<crate::db::training::EnduranceStats> {
    let obj = v?;
    if obj.is_null() {
        return None;
    }
    Some(crate::db::training::EnduranceStats {
        duration_sec: obj.get("duration_sec").and_then(|x| x.as_i64()),
        distance_m: obj.get("distance_m").and_then(|x| x.as_f64()),
        elevation_m: obj.get("elevation_m").and_then(|x| x.as_f64()),
        avg_hr: obj.get("avg_hr").and_then(|x| x.as_i64()),
        calories: obj.get("calories").and_then(|x| x.as_i64()),
    })
}

fn strength_set_item_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "exercise": {
                "type": "string",
                "enum": StrengthExercise::slugs(),
                "description": "Canonical lift slug from the catalog. Use other for unknown movements."
            },
            "set_index": { "type": "integer" },
            "reps": { "type": "integer" },
            "weight_kg": { "type": "number" }
        },
        "required": ["exercise"]
    })
}

fn strength_sets_json(sets: &[crate::db::training::StrengthSet]) -> serde_json::Value {
    serde_json::Value::Array(
        sets.iter()
            .map(|s| {
                serde_json::json!({
                    "exercise": s.exercise,
                    "set_index": s.set_index,
                    "reps": s.reps,
                    "weight_kg": s.weight_kg,
                })
            })
            .collect(),
    )
}

async fn parse_strength_sets_arg(
    ctx: &ToolCtx<'_>,
    v: Option<&serde_json::Value>,
) -> Vec<crate::db::training::NewStrengthSet> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let owned: Vec<(Option<String>, Option<String>)> = arr
        .iter()
        .map(|s| {
            (
                s.get("exercise")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                s.get("exercise_name")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
            )
        })
        .collect();
    let labels: Vec<(Option<&str>, Option<&str>)> = owned
        .iter()
        .map(|(a, b)| (a.as_deref(), b.as_deref()))
        .collect();
    let resolved = crate::ai::exercise_selection::resolve_labels(
        ctx.pool,
        ctx.user_id,
        ctx.api_key,
        ctx.embedding_model,
        &labels,
        ctx.request_id,
    )
    .await;
    arr.iter()
        .enumerate()
        .zip(resolved)
        .filter_map(|((i, s), exercise)| {
            Some(crate::db::training::NewStrengthSet::new(
                exercise?,
                s.get("set_index")
                    .and_then(|x| x.as_i64())
                    .unwrap_or(i as i64 + 1),
                s.get("reps").and_then(|x| x.as_i64()),
                s.get("weight_kg").and_then(|x| x.as_f64()),
            ))
        })
        .collect()
}

#[cfg(test)]
fn parse_strength_sets_exact(
    v: Option<&serde_json::Value>,
) -> Vec<crate::db::training::NewStrengthSet> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let exercise = StrengthExercise::parse_input(
                s.get("exercise").and_then(|x| x.as_str()),
                s.get("exercise_name").and_then(|x| x.as_str()),
            )?;
            Some(crate::db::training::NewStrengthSet::new(
                exercise,
                s.get("set_index")
                    .and_then(|x| x.as_i64())
                    .unwrap_or(i as i64 + 1),
                s.get("reps").and_then(|x| x.as_i64()),
                s.get("weight_kg").and_then(|x| x.as_f64()),
            ))
        })
        .collect()
}

fn training_entry_json(entry: &crate::db::training::TrainingEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "user_id": entry.user_id,
        "description": entry.description,
        "activity": entry.activity_slug,
        "recorded_at": entry.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        "endurance": entry.endurance,
        "strength_sets": strength_sets_json(&entry.strength_sets),
        "summary": crate::db::training::summary_line(entry),
    })
}

fn training_entry_card_json(entry: &crate::db::training::TrainingEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "activity": entry.activity_slug,
        "recorded_at": entry.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        "description": entry.description,
        "endurance": entry.endurance,
        "strength_sets": entry.strength_sets.iter().map(|s| {
            serde_json::json!({
                "slug": s.exercise.slug(),
                "set_index": s.set_index,
                "reps": s.reps,
                "weight_kg": s.weight_kg,
            })
        }).collect::<Vec<_>>(),
        "card": crate::db::training::format_card(entry),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tools_has_unique_names() {
        let tools = all_tools();
        let mut seen = std::collections::HashSet::new();
        for tool in &tools {
            assert!(
                seen.insert(tool.name.clone()),
                "duplicate tool name in catalog: {}",
                tool.name
            );
        }
    }

    #[test]
    fn all_tools_does_not_include_delete_task() {
        let names: Vec<_> = all_tools().into_iter().map(|t| t.name).collect();
        assert!(!names.iter().any(|n| n == "delete_task"));
        assert!(names.iter().any(|n| n == "remove_task"));
    }

    #[test]
    fn unknown_tool_response_lists_available_tools() {
        let available = vec!["remove_meal".into(), "log_meal".into()];
        let body = unknown_tool_response("delete_meal", &available);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(
            parsed["error"]
                .as_str()
                .unwrap()
                .starts_with("Unknown tool:")
        );
        assert_eq!(
            parsed["available_tools"],
            serde_json::json!(["remove_meal", "log_meal"])
        );
        assert!(parsed["hint"].is_string());
        assert!(is_unknown_tool_response(&body));
        assert!(!is_unknown_tool_response(r#"{"success":true}"#));
    }

    #[test]
    fn capability_catalog_lists_tools_without_itself() {
        let catalog = capability_catalog();
        assert!(catalog.contains("log_weight —"));
        assert!(catalog.contains("get_training_entries —"));
        assert!(catalog.contains("get_training_entry —"));
        assert!(catalog.contains("get_group_training_stats —"));
        assert!(catalog.contains("send_coach_image —"));
        assert!(catalog.contains("get_image_quota —"));
        assert!(catalog.contains("search_photos —"));
        assert!(
            catalog.contains("Do not use this to track physique"),
            "search_photos must not be presented as a physique-progress tool"
        );
        assert!(catalog.contains("search_foods —"));
        assert!(catalog.contains("register_food —"));
        assert!(catalog.contains("log_meal —"));
        assert!(catalog.contains("get_day_nutrition —"));
        assert!(catalog.contains("show_nutrition_table —"));
        assert!(catalog.contains("propose_meals —"));
        assert!(catalog.contains("remember_food_preference —"));
        assert!(!catalog.contains("list_capabilities"));
    }

    #[test]
    fn parse_strength_sets_uses_catalog_enum() {
        let v = serde_json::json!([
            {"exercise":"bench_press","reps":5,"weight_kg":80},
            {"exercise":"other","set_index":2,"reps":40},
            {"exercise_name":"Développé couché","reps":5}
        ]);
        let sets = parse_strength_sets_exact(Some(&v));
        assert_eq!(sets[0].exercise, StrengthExercise::BenchPress);
        assert_eq!(sets[1].exercise, StrengthExercise::Other);
        assert_eq!(sets[1].set_index, 2);
        assert_eq!(sets[2].exercise, StrengthExercise::BenchPress);
    }

    #[test]
    fn group_stats_window_accepts_known_periods() {
        assert!(group_stats_window("last").is_some());
        assert!(group_stats_window("4w").is_some());
        assert!(group_stats_window("3m").is_some());
        assert!(group_stats_window("year").is_none());
    }

    fn tool_ctx<'a>(
        pool: &'a crate::db::Db,
        user_id: i64,
        tz: &'a str,
        chat_id: Option<i64>,
    ) -> ToolCtx<'a> {
        ToolCtx {
            pool,
            user_id,
            timezone: tz,
            lang: crate::i18n::Lang::Fr,
            is_admin: false,
            bot: None,
            bot_token: "",
            chat_id,
            reply_to_message_id: None,
            api_key: None,
            embedding_model: "test",
            image_model: "gemini-2.5-flash-image",
            image_max_per_user_day: 2,
            user_photo: None,
            prompt_embedding: None,
            request_id: None,
            rag_enabled: false,
        }
    }

    #[tokio::test]
    async fn get_active_goals_includes_current_value() {
        use crate::db::goals::{GoalType, NewGoal, set_goal};
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};
        use crate::db::weight::{NewWeightEntry, insert as insert_weight};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99401, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        set_goal(
            &pool,
            NewGoal {
                user_id: user.id,
                goal_type: GoalType::Weight,
                target_value: 75.0,
            },
        )
        .await
        .unwrap();
        insert_weight(
            &pool,
            NewWeightEntry {
                user_id: user.id,
                weight_kg: 82.5,
                recorded_at: None,
            },
        )
        .await
        .unwrap();

        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let raw = execute_goals(&ctx).await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let goals = v["goals"].as_array().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0]["target_value"], 75.0);
        assert_eq!(goals[0]["current_value"], 82.5);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn same_item_names_ignores_quantity_prefix_and_accents() {
        assert!(same_item_names(
            &["Oeufs entiers".into(), "Pain complet".into()],
            &["3 Œufs entiers".into(), "Pain complet".into()],
        ));
        assert!(!same_item_names(
            &["Oeufs entiers".into()],
            &["Oeufs entiers".into(), "Banane".into()],
        ));
        assert!(!same_item_names(
            &["Banane".into()],
            &["Oeufs entiers".into()],
        ));
    }

    #[tokio::test]
    async fn log_meal_refuses_same_item_names_already_on_the_day() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99420, Role::Sportive, Some("m".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let first = serde_json::json!({
            "kind": "breakfast",
            "eaten_at": "today",
            "items": [{
                "name": "Oeufs entiers",
                "quantity": 3,
                "unit": "piece",
                "source": "estimated",
                "kcal": 215,
                "protein_g": 18,
                "carbs_g": 1,
                "fat_g": 15
            }]
        });
        let created = execute_log_meal(&ctx, &first).await;
        let created_v: serde_json::Value = serde_json::from_str(&created).unwrap();
        assert_eq!(created_v["success"], true);
        let meal_id = created_v["meal"]["id"].as_i64().unwrap();

        let again = serde_json::json!({
            "kind": "breakfast",
            "eaten_at": "today",
            "items": [{
                "name": "3 Œufs entiers",
                "quantity": 3,
                "unit": "piece",
                "source": "estimated",
                "kcal": 215,
                "protein_g": 18,
                "carbs_g": 1,
                "fat_g": 15
            }]
        });
        let refused = execute_log_meal(&ctx, &again).await;
        let refused_v: serde_json::Value = serde_json::from_str(&refused).unwrap();
        assert_eq!(refused_v["error"], "already logged");
        assert_eq!(refused_v["meal_id"], meal_id);
        assert!(
            refused_v["hint"]
                .as_str()
                .unwrap()
                .contains("Answer from this meal")
        );
        assert!(!refused_v["hint"].as_str().unwrap().contains("today"));
        assert_eq!(
            refused_v["date"].as_str().unwrap(),
            chrono::Utc::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        );

        let other = serde_json::json!({
            "kind": "snack",
            "eaten_at": "today",
            "items": [{
                "name": "Banane",
                "quantity": 1,
                "unit": "piece",
                "source": "estimated",
                "kcal": 90,
                "protein_g": 1,
                "carbs_g": 23,
                "fat_g": 0
            }]
        });
        let logged = execute_log_meal(&ctx, &other).await;
        let logged_v: serde_json::Value = serde_json::from_str(&logged).unwrap();
        assert_eq!(logged_v["success"], true);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn log_meal_refuses_catalog_food_id_already_on_that_day() {
        use crate::db::foods::insert as insert_food;
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99424, Role::Sportive, Some("m".into()), None, None)
            .await
            .unwrap();
        let food = insert_food(
            &pool,
            catalog_food(user.id, "Whey Isolate", "MyBrand", 370.0, 80.0),
        )
        .await
        .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let by_id = serde_json::json!({
            "kind": "snack",
            "eaten_at": "2026-09-20T08:00:00Z",
            "items": [{ "food_id": food.id, "quantity": 30, "unit": "g" }]
        });
        let created = execute_log_meal(&ctx, &by_id).await;
        let created_v: serde_json::Value = serde_json::from_str(&created).unwrap();
        assert_eq!(created_v["success"], true);
        let meal_id = created_v["meal"]["id"].as_i64().unwrap();

        let again = execute_log_meal(&ctx, &by_id).await;
        let again_v: serde_json::Value = serde_json::from_str(&again).unwrap();
        assert_eq!(again_v["error"], "already logged");
        assert_eq!(again_v["meal_id"], meal_id);
        assert_eq!(again_v["date"], "2026-09-20");
        let hint = again_v["hint"].as_str().unwrap();
        assert!(hint.contains("2026-09-20"));
        assert!(!hint.contains("today"));
        assert_eq!(again_v["items"][0]["food_id"], food.id);

        let by_name = serde_json::json!({
            "kind": "snack",
            "eaten_at": "2026-09-20T18:00:00Z",
            "items": [{ "name": "Whey Isolate", "quantity": 30, "unit": "g" }]
        });
        let named = execute_log_meal(&ctx, &by_name).await;
        let named_v: serde_json::Value = serde_json::from_str(&named).unwrap();
        assert_eq!(named_v["error"], "already logged");
        assert_eq!(named_v["meal_id"], meal_id);

        let other_day = serde_json::json!({
            "kind": "snack",
            "eaten_at": "2026-09-21T08:00:00Z",
            "items": [{ "food_id": food.id, "quantity": 30, "unit": "g" }]
        });
        let next = execute_log_meal(&ctx, &other_day).await;
        let next_v: serde_json::Value = serde_json::from_str(&next).unwrap();
        assert_eq!(next_v["success"], true);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn group_training_stats_requires_group_chat() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99402, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let raw = execute_group_training_stats(&ctx, &serde_json::json!({})).await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let err = v["error"].as_str().unwrap();
        assert!(err.contains("not the group"));
        assert!(err.contains("Do not invent"));
        let dm = tool_ctx(&pool, user.id, "UTC", Some(42));
        let dm_raw = execute_group_training_stats(&dm, &serde_json::json!({"period": "4w"})).await;
        let dm_err: serde_json::Value = serde_json::from_str(&dm_raw).unwrap();
        assert!(
            dm_err["error"]
                .as_str()
                .unwrap()
                .contains("Tell the user to ask in the group")
        );
        let empty_group = tool_ctx(&pool, user.id, "UTC", Some(-100_42));
        let empty_raw = execute_group_training_stats(&empty_group, &serde_json::json!({})).await;
        let empty_err: serde_json::Value = serde_json::from_str(&empty_raw).unwrap();
        assert!(
            empty_err["error"]
                .as_str()
                .unwrap()
                .contains("Do not invent a session count")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn group_training_stats_returns_ranking() {
        use crate::db::tests::test_db;
        use crate::db::training::{NewTrainingEntry, insert as insert_training};
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let a = insert_user(
            &pool,
            99403,
            Role::Sportive,
            Some("alice".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let b = insert_user(&pool, 99404, Role::Sportive, Some("bob".into()), None, None)
            .await
            .unwrap();
        let chat_id = -100_99403;
        crate::db::group_members::upsert(&pool, chat_id, a.telegram_user_id)
            .await
            .unwrap();
        crate::db::group_members::upsert(&pool, chat_id, b.telegram_user_id)
            .await
            .unwrap();
        insert_training(&pool, NewTrainingEntry::simple(a.id, "Run".into(), None))
            .await
            .unwrap();
        let earlier = chrono::Utc::now() - chrono::Duration::hours(3);
        insert_training(
            &pool,
            NewTrainingEntry::simple(a.id, "Gym".into(), Some(earlier)),
        )
        .await
        .unwrap();
        insert_training(&pool, NewTrainingEntry::simple(b.id, "Ride".into(), None))
            .await
            .unwrap();

        let ctx = tool_ctx(&pool, a.id, "UTC", Some(chat_id));
        let raw = execute_group_training_stats(&ctx, &serde_json::json!({"period": "4w"})).await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["period"], "4w");
        let ranking = v["ranking"].as_array().unwrap();
        assert_eq!(ranking.len(), 2);
        assert_eq!(ranking[0]["name"], "@alice");
        assert_eq!(ranking[0]["sessions"], 2);
        assert_eq!(ranking[1]["name"], "@bob");
        assert_eq!(ranking[1]["sessions"], 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn send_coach_image_refuses_without_chat() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99410, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let raw = execute_send_coach_image(
            &ctx,
            &serde_json::json!({
                "kind": "victory",
                "scene": "bronze statue of a runner"
            }),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("telegram")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn send_coach_image_edit_requires_photo() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99411, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", Some(1));
        let raw = execute_send_coach_image(
            &ctx,
            &serde_json::json!({
                "kind": "edit",
                "scene": "turn the photo into a statue",
                "use_user_photo": true
            }),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("photo")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn send_coach_image_refuses_when_quota_exhausted() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99412, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        assert_eq!(
            crate::db::ai_usage::count_today_for_model(&pool, user.id, "gemini-2.5-flash-image")
                .await
                .unwrap(),
            0
        );
        for _ in 0..2 {
            crate::db::ai_usage::insert(&pool, user.id, "gemini-2.5-flash-image", 0, 0, 0)
                .await
                .unwrap();
        }
        assert_eq!(
            crate::db::ai_usage::count_today_for_model(&pool, user.id, "gemini-2.5-flash-image")
                .await
                .unwrap(),
            2
        );
        let ctx = tool_ctx(&pool, user.id, "UTC", Some(1));
        let raw = execute_send_coach_image(
            &ctx,
            &serde_json::json!({
                "kind": "victory",
                "scene": "official poster"
            }),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["error"], "quota_exceeded");
        assert_eq!(v["quota_exceeded"], true);
        assert_eq!(v["max_per_day"], 2);
        assert_eq!(v["used_today"], 2);
        assert_eq!(v["remaining_today"], 0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn get_image_quota_reports_used_and_remaining() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99413, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let v: serde_json::Value =
            serde_json::from_str(&execute_get_image_quota(&ctx).await).unwrap();
        assert_eq!(v["max_per_day"], 2);
        assert_eq!(v["used_today"], 0);
        assert_eq!(v["remaining_today"], 2);
        crate::db::ai_usage::insert(&pool, user.id, "gemini-2.5-flash-image", 0, 0, 0)
            .await
            .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&execute_get_image_quota(&ctx).await).unwrap();
        assert_eq!(v["used_today"], 1);
        assert_eq!(v["remaining_today"], 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn search_photos_requires_query_or_prompt_embedding() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99414, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", Some(user.telegram_user_id));
        let raw = execute_search_photos(&ctx, &serde_json::json!({})).await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v["error"].as_str().unwrap().contains("query"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn search_photos_returns_hits_from_prompt_embedding() {
        use crate::db::photo::{save_photo_message, upsert_embedding};
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};
        use crate::embeddings::EMBEDDING_DIMENSION;

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99415, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let mut emb = vec![0.0f32; EMBEDDING_DIMENSION];
        emb[3] = 1.0;
        let id = save_photo_message(
            &pool,
            user.telegram_user_id,
            7,
            user.telegram_user_id,
            Some("strava".into()),
            None,
            Some("2026-08-16 10:00:00"),
        )
        .await
        .unwrap();
        upsert_embedding(&pool, id, &emb).await.unwrap();

        let mut ctx = tool_ctx(&pool, user.id, "UTC", Some(user.telegram_user_id));
        ctx.prompt_embedding = Some(&emb);
        let raw = execute_search_photos(&ctx, &serde_json::json!({"limit": 3})).await;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let photos = v["photos"].as_array().unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0]["photo_id"], id);
        assert_eq!(photos[0]["taken_at"], "2026-08-16 10:00:00");
        assert_eq!(photos[0]["caption"], "strava");
        let _ = std::fs::remove_file(&path);
    }

    fn catalog_food(
        user_id: i64,
        name: &str,
        brand: &str,
        kcal_per_100: f64,
        protein_g_per_100: f64,
    ) -> crate::db::foods::NewFood {
        crate::db::foods::NewFood {
            user_id,
            name: name.into(),
            brand: Some(brand.into()),
            barcode: None,
            notes: None,
            kcal_per_100,
            protein_g_per_100,
            carbs_g_per_100: 4.0,
            fat_g_per_100: 0.5,
            serving_qty: 100.0,
            serving_unit: crate::db::foods::QuantityUnit::G,
            serving_label: None,
            source: crate::db::foods::FoodSource::Manual,
        }
    }

    #[tokio::test]
    async fn resolve_new_item_ambiguous_name_errors_even_with_macros() {
        use crate::db::foods::insert as insert_food;
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99420, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        insert_food(
            &pool,
            catalog_food(user.id, "Yogurt Greek", "Fage", 97.0, 9.0),
        )
        .await
        .unwrap();
        insert_food(
            &pool,
            catalog_food(user.id, "Yogurt Skyr", "Arla", 63.0, 11.0),
        )
        .await
        .unwrap();

        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let err = resolve_new_item(
            &ctx,
            &serde_json::json!({
                "name": "Yogurt",
                "quantity": 150.0,
                "unit": "g",
                "kcal": 200.0,
                "protein_g": 10.0,
                "carbs_g": 20.0,
                "fat_g": 5.0
            }),
        )
        .await
        .unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .contains("Ambiguous food name 'Yogurt'")
        );
        assert_eq!(v["candidates"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn resolve_new_item_unique_name_prefers_catalog_over_tool_macros() {
        use crate::db::foods::insert as insert_food;
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99421, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let food = insert_food(
            &pool,
            catalog_food(user.id, "Whey Isolate", "MyBrand", 370.0, 80.0),
        )
        .await
        .unwrap();

        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let item = resolve_new_item(
            &ctx,
            &serde_json::json!({
                "name": "Whey Isolate",
                "quantity": 30.0,
                "unit": "g",
                "kcal": 999.0,
                "protein_g": 1.0,
                "carbs_g": 1.0,
                "fat_g": 1.0
            }),
        )
        .await
        .unwrap();
        assert_eq!(item.food_id, Some(food.id));
        assert_eq!(item.source, crate::db::meals::ItemSource::Catalog);
        assert_eq!(item.macros.kcal, 111.0);
        assert_eq!(item.macros.protein_g, 24.0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn resolve_new_item_estimated_source_uses_macros_despite_catalog_hits() {
        use crate::db::foods::insert as insert_food;
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99422, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        insert_food(
            &pool,
            catalog_food(user.id, "Yogurt Greek", "Fage", 97.0, 9.0),
        )
        .await
        .unwrap();
        insert_food(
            &pool,
            catalog_food(user.id, "Yogurt Skyr", "Arla", 63.0, 11.0),
        )
        .await
        .unwrap();

        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let item = resolve_new_item(
            &ctx,
            &serde_json::json!({
                "name": "Yogurt",
                "quantity": 150.0,
                "unit": "g",
                "source": "estimated",
                "kcal": 180.0,
                "protein_g": 12.0,
                "carbs_g": 16.0,
                "fat_g": 6.0
            }),
        )
        .await
        .unwrap();
        assert_eq!(item.food_id, None);
        assert_eq!(item.source, crate::db::meals::ItemSource::Estimated);
        assert_eq!(item.macros.kcal, 180.0);
        assert_eq!(item.description, "Yogurt");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn resolve_new_item_unknown_name_with_macros_is_estimated_not_catalog() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99423, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let item = resolve_new_item(
            &ctx,
            &serde_json::json!({
                "name": "street falafel",
                "quantity": 1.0,
                "unit": "piece",
                "kcal": 450.0,
                "protein_g": 15.0,
                "carbs_g": 40.0,
                "fat_g": 22.0
            }),
        )
        .await
        .unwrap();
        assert_eq!(item.food_id, None);
        assert_eq!(item.source, crate::db::meals::ItemSource::Estimated);
        assert_eq!(item.macros.kcal, 450.0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn remember_and_propose_meals_from_catalog() {
        use crate::db::foods::{FoodSource, NewFood, QuantityUnit, insert as insert_food};
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99430, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        insert_food(
            &pool,
            NewFood {
                user_id: user.id,
                name: "Whey Isolate".into(),
                brand: None,
                barcode: None,
                notes: None,
                kcal_per_100: 370.0,
                protein_g_per_100: 80.0,
                carbs_g_per_100: 6.0,
                fat_g_per_100: 4.0,
                serving_qty: 30.0,
                serving_unit: QuantityUnit::G,
                serving_label: Some("1 scoop".into()),
                source: FoodSource::Manual,
            },
        )
        .await
        .unwrap();
        insert_food(
            &pool,
            NewFood {
                user_id: user.id,
                name: "Tuna".into(),
                brand: None,
                barcode: None,
                notes: None,
                kcal_per_100: 116.0,
                protein_g_per_100: 26.0,
                carbs_g_per_100: 0.0,
                fat_g_per_100: 1.0,
                serving_qty: 100.0,
                serving_unit: QuantityUnit::G,
                serving_label: None,
                source: FoodSource::Manual,
            },
        )
        .await
        .unwrap();

        let ctx = tool_ctx(&pool, user.id, "UTC", None);
        let saved = execute_remember_food_preference(
            &ctx,
            &serde_json::json!({"kind": "dislike", "text": "tuna"}),
        )
        .await;
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved["success"], true);

        let proposed =
            execute_propose_meals(&ctx, &serde_json::json!({"envie": "whey", "kind": "snack"}))
                .await;
        let proposed: serde_json::Value = serde_json::from_str(&proposed).unwrap();
        assert_eq!(proposed["training_bias"], "rest");
        let options = proposed["options"].as_array().unwrap();
        assert!(!options.is_empty());
        let titles = options
            .iter()
            .filter_map(|o| o["title"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(titles.to_lowercase().contains("whey"));
        assert!(!titles.to_lowercase().contains("tuna"));

        let group = tool_ctx(&pool, user.id, "UTC", Some(-100));
        let refused = execute_propose_meals(&group, &serde_json::json!({"envie": "whey"})).await;
        assert!(refused.contains("private"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn log_weight_rejects_hallucinated_one_kg() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};
        use crate::db::weight::list_for_user as list_weight;

        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99440, Role::Sportive, Some("w".into()), None, None)
            .await
            .unwrap();
        let ctx = tool_ctx(&pool, user.id, "UTC", None);

        let rejected = execute_log_weight(&ctx, &serde_json::json!({"weight_kg": 1.0})).await;
        let rejected: serde_json::Value = serde_json::from_str(&rejected).unwrap();
        assert!(
            rejected["error"]
                .as_str()
                .unwrap()
                .contains("realistic body weight")
        );
        assert!(list_weight(&pool, user.id).await.unwrap().is_empty());

        let ok = execute_log_weight(&ctx, &serde_json::json!({"weight_kg": 82.5})).await;
        let ok: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok["success"], true);
        assert_eq!(ok["weight_entry"]["weight_kg"], 82.5);
        let _ = std::fs::remove_file(&path);
    }
}
