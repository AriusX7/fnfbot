use poise::serenity_prelude::{self as serenity, Context, Reaction};
use tracing::info;

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

    if reaction.emoji.unicode_eq("✅") {
        handle_add_user(data, message_id, user_id).await?;
        info!("signed up user {} for room {}", user_id, message_id.0);
    } else if reaction.emoji.unicode_eq("❌") {
        handle_remove_user(data, message_id, user_id).await?;
        info!("removed user {} from room {}", user_id, message_id.0);
    } else {
        return Ok(());
    }

    // try to remove the reaction
    channel_id
        .delete_reaction(ctx, message_id, Some(user_id), reaction.emoji.clone())
        .await?;

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
    data: &Data,
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
) -> Result<(), Error> {
    let record = sqlx::query!(
        "SELECT COUNT(*) FROM signup WHERE message_id = $1",
        message_id.0 as i64
    )
    .fetch_one(&data.db_pool)
    .await?;

    if record.count.unwrap_or_default() >= 15 {
        return Ok(());
    }

    sqlx::query!(
        "INSERT INTO signup (message_id, user_id) VALUES ($1, $2)
            ON CONFLICT (message_id, user_id) DO NOTHING",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    Ok(())
}

async fn handle_remove_user(
    data: &Data,
    message_id: serenity::MessageId,
    user_id: serenity::UserId,
) -> Result<(), Error> {
    sqlx::query!(
        "DELETE FROM signup WHERE message_id = $1 AND user_id = $2",
        message_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    Ok(())
}
