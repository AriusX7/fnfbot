use poise::serenity_prelude::{self as serenity, Context, Reaction};
use tracing::{error, info};

use crate::{Data, Error, EMBED_COLOUR, REACT_STR};

pub async fn handle_on_raw_reaction(
    reaction: &Reaction,
    ctx: &Context,
    data: &Data,
    bot_id: u64,
) -> Result<(), Error> {
    let guild_id = match reaction.guild_id {
        Some(gid) => gid,
        _ => return Ok(()),
    };

    let user_id = match reaction.user_id {
        Some(uid) if uid != bot_id => uid,
        _ => return Ok(()),
    };

    let channel_id = reaction.channel_id;

    if data
        .guild_configs
        .get(&guild_id.0)
        .filter(|f| f.channel_id == Some(channel_id.0))
        .is_none()
    {
        return Ok(());
    }

    let message_id = reaction.message_id;

    if !data.messages.contains(&message_id.0) {
        return Ok(());
    }

    // try to remove the reaction
    channel_id
        .delete_reaction(ctx, message_id, Some(user_id), reaction.emoji.clone())
        .await?;

    if reaction.emoji.unicode_eq("✅") {
        if !handle_add_user(ctx, data, message_id, user_id).await? {
            return Ok(());
        }
        info!("registered user {} for room {}", user_id, message_id.0);
    } else if reaction.emoji.unicode_eq("❌") {
        if !handle_remove_user(ctx, data, message_id, user_id).await? {
            return Ok(());
        }
        info!("deregistered user {} from room {}", user_id, message_id.0);
    } else {
        return Ok(());
    }

    // update message footer with new number of signups
    let mut msg = if let Some(m) = ctx.cache.message(channel_id, message_id) {
        m
    } else {
        channel_id.message(&ctx, message_id).await?
    };

    if let Some(e) = msg.embeds.first() {
        let mut embed = serenity::CreateEmbed::default();

        embed.colour(e.colour.unwrap_or_else(|| EMBED_COLOUR.into()));

        if let Some(ref title) = e.title {
            embed.title(title);
        }

        if let Some(ref desc) = e.description {
            embed.description(desc);
        }

        let record = sqlx::query!(
            "SELECT COUNT(*) FROM signup WHERE message_id = $1",
            message_id.0 as i64
        )
        .fetch_one(&data.db_pool)
        .await?;

        embed.footer(|f| {
            f.text(format!(
                "{}/15 spots available | {REACT_STR}",
                15 - record.count.unwrap_or(0)
            ))
        });
        msg.edit(&ctx, |m| m.set_embed(embed)).await?;
    }

    Ok(())
}

async fn handle_add_user(
    ctx: &Context,
    data: &Data,
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
) -> Result<bool, Error> {
    if let Some((mid, num)) = check_if_registered_any(user_id, data).await? {
        if message_id == mid as u64 {
            dm_user(ctx, user_id, "You are already registered for this room.").await?;
        } else {
            dm_user(
                ctx,
                user_id,
                format!(
                    "You can only register for one room. \
                    You are currently registered for room #{}.",
                    num
                ),
            )
            .await?;
        }
        return Ok(false);
    }

    let record = sqlx::query!(
        "SELECT COUNT(*) FROM signup WHERE message_id = $1",
        message_id.0 as i64
    )
    .fetch_one(&data.db_pool)
    .await?;

    let count = record.count.unwrap_or_default();
    if count >= 15 {
        return Ok(false);
    }

    let mut dm = match dm_user(ctx, user_id, "Registering...").await {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };

    if let Err(e) = sqlx::query!(
        "INSERT INTO signup (message_id, user_id) VALUES ($1, $2)
                ON CONFLICT (message_id, user_id) DO NOTHING",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await
    {
        dm.edit(&ctx, |f| {
            f.content(format!(
                "There was an error registering. Please contact \
                an DBC Sheriff with the following error:\n\n```{e}```"
            ))
        })
        .await?;
        error!("error registering user: {e}");
    };

    dm.edit(ctx, |m| {
        if count < 9 {
            m.content("You registered for the room.")
        } else {
            m.content(format!(
                "You registered as a reserve for the room. Your position is {}/6.",
                count - 9 + 1
            ))
        }
    })
    .await?;

    Ok(true)
}

async fn handle_remove_user(
    ctx: &Context,
    data: &Data,
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
) -> Result<bool, Error> {
    if !check_if_registered(message_id, user_id, data).await? {
        return Ok(false);
    }

    let mut dm = match dm_user(ctx, user_id, "Deregistering...").await {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };

    if let Err(e) = sqlx::query!(
        "DELETE FROM signup WHERE message_id = $1 AND user_id = $2",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await
    {
        dm.edit(&ctx, |f| {
            f.content(format!(
                "There was an error deregistering. Please contact \
                an FNF Host with the following error:\n\n```{e}```"
            ))
        })
        .await?;
        error!("error deregistering user: {e}");
    };

    dm.edit(ctx, |m| m.content("You have deregistered from the room."))
        .await?;

    Ok(true)
}

async fn check_if_registered(
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
    data: &Data,
) -> Result<bool, Error> {
    let record = sqlx::query!(
        r#"SELECT exists (SELECT 1 FROM signup WHERE message_id = $1 AND user_id = $2) as "exists!""#,
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .fetch_one(&data.db_pool)
    .await?;
    Ok(record.exists)
}

async fn check_if_registered_any(
    user_id: serenity::UserId,
    data: &Data,
) -> Result<Option<(i64, i32)>, Error> {
    let record = sqlx::query!(
        "SELECT m.message_id, num FROM signup s JOIN message m ON user_id = $1 AND s.message_id = m.message_id;",
        user_id.0 as i64,
    )
    .fetch_optional(&data.db_pool)
    .await?;
    Ok(record.map(|r| (r.message_id, r.num)))
}

async fn dm_user(
    ctx: &Context,
    user_id: serenity::UserId,
    content: impl std::fmt::Display,
) -> Result<serenity::Message, Error> {
    match user_id.create_dm_channel(&ctx).await {
        Ok(c) => match c.say(&ctx, content).await {
            Ok(m) => Ok(m),
            Err(e) => {
                error!("unable to dm user {}, error: {}", user_id, e);
                Err(e.into())
            },
        },
        Err(e) => {
            error!(
                "unable to create dm channel with user {}, error: {}",
                user_id, e
            );
            Err(e.into())
        },
    }
}
