use poise::serenity_prelude::{self as serenity, Context, Reaction};
use tracing::info;

use crate::{Data, Error, EMBED_COLOUR, REACT_STR};

pub async fn handle_on_raw_reaction(
    reaction: &Reaction,
    ctx: &Context,
    data: &Data,
    bot_id: u64,
) -> Result<(), Error> {
    if reaction.guild_id.is_none() {
        return Ok(());
    }

    let user_id = match reaction.user_id {
        Some(uid) if uid != bot_id => uid,
        _ => return Ok(()),
    };

    let channel_id = reaction.channel_id;

    {
        match data
            .channels_and_messages
            .lock()
            .unwrap()
            .get(&channel_id.0)
        {
            Some(&msg_id) if msg_id == reaction.message_id.0 => (),
            _ => return Ok(()),
        }
    }

    if reaction.emoji.unicode_eq("✅") {
        handle_add_user(data, channel_id, user_id).await?;
        info!("signed up user {} for room {}", user_id, channel_id.0);
    } else if reaction.emoji.unicode_eq("❌") {
        handle_remove_user(data, channel_id, user_id).await?;
        info!("removed user {} from room {}", user_id, channel_id.0);
    } else {
        return Ok(());
    }

    // try to remove the reaction
    channel_id
        .delete_reaction(
            ctx,
            reaction.message_id,
            Some(user_id),
            reaction.emoji.clone(),
        )
        .await?;

    // update message footer with new number of signups
    let mut msg = if let Some(m) = ctx.cache.message(channel_id, reaction.message_id) {
        m
    } else {
        channel_id.message(&ctx, reaction.message_id).await?
    };

    let embed = if let Some(e) = msg.embeds.first() {
        let mut embed = serenity::CreateEmbed::default();

        embed.colour(e.colour.unwrap_or_else(|| EMBED_COLOUR.into()));

        if let Some(ref desc) = e.description {
            embed.description(desc.clone());
        }

        let record = sqlx::query!(
            "SELECT COUNT(*) FROM react WHERE channel_id = $1",
            channel_id.0 as i64
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
    channel_id: serenity::ChannelId,
    user_id: serenity::UserId,
) -> Result<(), Error> {
    let record = sqlx::query!(
        "SELECT COUNT(*) FROM react WHERE channel_id = $1",
        channel_id.0 as i64
    )
    .fetch_one(&data.db_pool)
    .await?;

    if record.count.unwrap_or_default() >= 15 {
        return Ok(());
    }

    sqlx::query!(
        "INSERT INTO react (channel_id, user_id) VALUES ($1, $2)
            ON CONFLICT (channel_id, user_id) DO NOTHING",
        channel_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    Ok(())
}

async fn handle_remove_user(
    data: &Data,
    channel_id: serenity::ChannelId,
    user_id: serenity::UserId,
) -> Result<(), Error> {
    sqlx::query!(
        "DELETE FROM react WHERE channel_id = $1 AND user_id = $2",
        channel_id.0 as i64,
        user_id.0 as i64,
    )
    .execute(&data.db_pool)
    .await?;

    Ok(())
}

pub async fn handle_on_channel_delete(channel_id: u64, data: &Data) -> Result<(), Error> {
    {
        if !data
            .channels_and_messages
            .lock()
            .unwrap()
            .contains_key(&channel_id)
        {
            return Ok(());
        }
    }

    sqlx::query!(
        "DELETE FROM channel_message WHERE channel_id = $1",
        channel_id as i64
    )
    .execute(&data.db_pool)
    .await?;

    // only remove from our local cache if database removal is successful
    {
        data.channels_and_messages
            .lock()
            .unwrap()
            .remove(&channel_id);
    }

    info!("removed room {}", channel_id);

    Ok(())
}
