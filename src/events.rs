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

    {
        if data
            .guild_configs
            .lock()
            .unwrap()
            .get(&guild_id.0)
            .filter(|f| f.channel_id == Some(channel_id.0))
            .is_none()
        {
            return Ok(());
        }
    }

    let message_id = reaction.message_id;

    {
        if !data.messages.lock().unwrap().contains(&message_id.0) {
            return Ok(());
        }
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

    let embed = if let Some(e) = msg.embeds.first() {
        let mut embed = serenity::CreateEmbed::default();

        embed.colour(e.colour.unwrap_or_else(|| EMBED_COLOUR.into()));

        if let Some(ref desc) = e.description {
            embed.description(desc.clone());
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

        embed
    } else {
        return Ok(());
    };

    msg.edit(&ctx, |m| m.set_embed(embed)).await?;

    Ok(())
}

async fn handle_add_user(
    ctx: &Context,
    data: &Data,
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
) -> Result<bool, Error> {
    if check_if_registered(message_id, user_id, data).await? {
        dm_user(
            ctx,
            user_id,
            "You are already registered for this FNF room.",
        )
        .await?;
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

    sqlx::query!(
        "INSERT INTO signup (message_id, user_id) VALUES ($1, $2)
            ON CONFLICT (message_id, user_id) DO NOTHING",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    if count < 9 {
        dm_user(ctx, user_id, "You registered for the FNF room.").await?;
    } else {
        dm_user(
            ctx,
            user_id,
            format!(
                "You registered as a reserve for the FNF room. Your position is {}/6.",
                count - 9 + 1
            ),
        )
        .await?;
    }

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

    sqlx::query!(
        "DELETE FROM signup WHERE message_id = $1 AND user_id = $2",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    dm_user(ctx, user_id, "You have deregistered from the FNF room.").await?;

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

async fn dm_user(
    ctx: &Context,
    user_id: serenity::UserId,
    content: impl std::fmt::Display,
) -> Result<(), Error> {
    match user_id.create_dm_channel(&ctx).await {
        Ok(c) => {
            if let Err(e) = c.say(&ctx, content).await {
                error!("unable to dm user {}, error: {}", user_id, e);
            };
        },
        Err(e) => {
            error!(
                "unable to create dm channel with user {}, error: {}",
                user_id, e
            );
        },
    }

    Ok(())
}
