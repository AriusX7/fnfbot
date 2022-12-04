mod commands;
mod events;
mod utils;

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Mutex;
use std::time::Duration;

use poise::serenity_prelude::oauth::Scope;
use poise::serenity_prelude::{self as serenity, Permissions};
use poise::Event;
use tracing::{error, info, instrument, trace};

pub const REACT_STR: &str = "react to this message to register";
pub const EMBED_COLOUR: u32 = 0x007FB3;

// Types used by all command functions
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

/// Custom user data passed to all command functions
#[derive(Debug)]
pub struct Data {
    db_pool: sqlx::PgPool,
    messages: Mutex<HashSet<u64>>,
    guild_configs: Mutex<HashMap<u64, GuildConfig>>,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct GuildConfig {
    channel_id: Option<u64>,
    host_id: Option<u64>,
}

/// Show this help menu
#[poise::command(prefix_command, track_edits, slash_command)]
async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            extra_text_at_bottom: "Type ?help command for more info on a command.\n\
                You can edit your message to the bot and the bot will edit its response.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

/// Registers or unregisters application commands in this guild or globally
#[poise::command(prefix_command, hide_in_help)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;

    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx } => {
            error!("error in command `{}`: {:?}", ctx.command().name, error);
            if let Err(err) = ctx.say(error.to_string()).await {
                error!(
                    "error when sending a message in {}: {:?}",
                    ctx.channel_id(),
                    err
                );
            }
        },
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("error while handling error: {}", e)
            }
        },
    }
}

async fn get_all_messages(pool: &sqlx::PgPool) -> Result<HashSet<u64>, Error> {
    let res = sqlx::query!("SELECT * FROM message")
        .fetch_all(pool)
        .await?;

    Ok(res.iter().map(|r| (r.message_id as u64)).collect())
}

async fn get_guild_configs(pool: &sqlx::PgPool) -> Result<HashMap<u64, GuildConfig>, Error> {
    let res = sqlx::query!("SELECT * FROM config").fetch_all(pool).await?;

    Ok(res
        .iter()
        .map(|r| {
            (r.guild_id as u64, GuildConfig {
                channel_id: r.fnf_channel_id.map(|i| i as u64),
                host_id: r.host_role_id.map(|i| i as u64),
            })
        })
        .collect())
}

async fn invite_url(
    user: &serenity::CurrentUser,
    http: impl AsRef<serenity::Http>,
) -> Result<String, Error> {
    Ok(user
        .invite_url_with_oauth2_scopes(
            &http,
            Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::MANAGE_MESSAGES
                | Permissions::ADD_REACTIONS
                | Permissions::EMBED_LINKS
                | Permissions::ATTACH_FILES
                | Permissions::READ_MESSAGE_HISTORY
                | Permissions::USE_SLASH_COMMANDS,
            &[Scope::Bot, Scope::ApplicationsCommands],
        )
        .await?)
}

#[instrument]
async fn app() -> Result<(), Error> {
    let options = poise::FrameworkOptions {
        commands: vec![
            help(),
            register(),
            commands::host(),
            commands::registrations(),
            commands::sethost(),
            commands::shutdown(),
            commands::remove(),
            commands::fnfchannel(),
            commands::removeall(),
        ],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("~".into()),
            edit_tracker: Some(poise::EditTracker::for_timespan(Duration::from_secs(60))),
            case_insensitive_commands: true,
            mention_as_prefix: true,
            ..Default::default()
        },
        on_error: |error| Box::pin(on_error(error)),
        pre_command: |ctx| {
            Box::pin(async move {
                trace!("Executing command {}...", ctx.command().qualified_name);
            })
        },
        post_command: |ctx| {
            Box::pin(async move {
                trace!("Executed command {}!", ctx.command().qualified_name);
            })
        },
        event_handler: |ctx, event, framework, data| {
            Box::pin(async move {
                match event {
                    Event::ReactionAdd { add_reaction } => {
                        events::handle_on_raw_reaction(add_reaction, ctx, data, framework.bot_id.0)
                            .await?
                    },
                    Event::Ready { data_about_bot } => {
                        info!("Connected as {}", data_about_bot.user.tag());
                        info!(
                            "Invite URL = {}",
                            invite_url(&data_about_bot.user, ctx).await?
                        );
                    },
                    _ => (),
                }

                Ok(())
            })
        },
        ..Default::default()
    };

    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&env::var("DATABASE_URL").expect("Missing `DATABASE_URL` env var."))
        .await?;
    sqlx::migrate!("./migrations").run(&db_pool).await?;

    let messages = Mutex::new(get_all_messages(&db_pool).await?);
    let guild_configs = Mutex::new(get_guild_configs(&db_pool).await?);

    let framework = poise::Framework::builder()
        .token(env::var("DISCORD_TOKEN").expect("Missing `DISCORD_TOKEN` env var."))
        .setup(move |ctx, _ready, _framework| {
            Box::pin(async move {
                ctx.set_activity(serenity::Activity::playing("Friday Night Friendlies"))
                    .await;

                Ok(Data {
                    db_pool,
                    messages,
                    guild_configs,
                })
            })
        })
        .initialize_owners(true)
        .options(options)
        .intents(
            serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT,
        )
        .client_settings(|c| c.cache_settings(|s| s.max_messages(100)))
        .build()
        .await?;

    let shard_manager = framework.shard_manager().clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    framework.start().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("unable to load .env file");
    tracing_subscriber::fmt::init();

    if let Err(e) = app().await {
        error!("error starting bot: {}", e);
    };
}
