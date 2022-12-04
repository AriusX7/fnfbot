use poise::serenity_prelude::{self as serenity, CacheHttp, Mentionable};
use tracing::error;

use crate::utils::{get_message_link, ParseableMessageId};
use crate::{invite_url, Context, Error, GuildConfig, EMBED_COLOUR, REACT_STR};

/// Set up self-role reaction message for a new room.
#[poise::command(prefix_command, guild_only, check = "is_host")]
pub async fn host(
    ctx: Context<'_>,
    #[description = "Date for the room"] date: String,
    #[description = "Time for the room"]
    #[rest]
    time: String,
) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => return Ok(()),
    };

    let channel = {
        if let Some(id) = ctx
            .data()
            .guild_configs
            .lock()
            .unwrap()
            .get(&guild_id.0)
            .copied()
            .unwrap_or_default()
            .channel_id
        {
            serenity::ChannelId(id)
        } else {
            return Err("fnf channel not set".into());
        }
    };

    let msg = channel
        .send_message(&ctx, |m| {
            m.embed(|e| {
                e.colour(EMBED_COLOUR)
                    .description(format!(
                        "{} is hosting a room at **{time}** on **{date}!**",
                        ctx.author().mention()
                    ))
                    .footer(|f| f.text(format!("15/15 spots available | {REACT_STR}")))
            })
        })
        .await?;

    msg.react(&ctx, '✅').await?;
    msg.react(&ctx, '❌').await?;

    sqlx::query!(
        "INSERT INTO message VALUES($1) ON CONFLICT (message_id) DO NOTHING",
        msg.id.0 as i64,
    )
    .execute(&ctx.data().db_pool)
    .await?;

    {
        let mut messages = ctx.data().messages.lock().unwrap();
        messages.insert(msg.id.0);
    }

    ctx.say("Self-role reaction message was set up successfully.")
        .await?;

    Ok(())
}

/// Shows the users that signed up for the room.
#[poise::command(prefix_command, aliases("reacts"), guild_only, check = "is_host")]
pub async fn signups(
    ctx: Context<'_>,
    #[description = "Message link for the room"]
    #[rest]
    message_id: ParseableMessageId,
) -> Result<(), Error> {
    let records = sqlx::query!(
        "SELECT user_id FROM signup WHERE message_id = $1 ORDER BY react_num",
        message_id.0 as i64
    )
    .fetch_all(&ctx.data().db_pool)
    .await?;

    let mut embed = serenity::CreateEmbed::default();
    embed.colour(EMBED_COLOUR);

    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => unreachable!(),
    };

    let desc_start = if let Some(link) = get_message_link(message_id.0, ctx.data(), guild_id) {
        format!("**Signups for Room [{}]({})**", message_id.0, link)
    } else {
        format!("**Signups for Room {}**", message_id.0,)
    };

    if records.is_empty() {
        embed.description(format!("{desc_start}\n\nNo signups yet."));
    } else {
        embed.description(desc_start);
        let mut registered = String::new();
        let mut reserves = String::new();

        for (i, record) in records.iter().enumerate() {
            if i < 9 {
                registered.push_str(&format_user_str(record.user_id));
            } else {
                reserves.push_str(&format_user_str(record.user_id));
            }
        }

        embed.field(
            format!("Registered ({}/9)", records.len().min(9)),
            registered,
            true,
        );

        if records.len() > 9 {
            embed.field(
                format!("Reserves ({}/6)", records.len() - 9),
                reserves,
                true,
            );
        }
    }

    ctx.send(|m| {
        m.embed(|e| {
            e.0 = embed.0;
            e
        })
    })
    .await?;

    Ok(())
}

/// Sets the host role ID.
#[poise::command(prefix_command, owners_only, guild_only)]
pub async fn sethost(
    ctx: Context<'_>,
    #[description = "The host role id"] id: u64,
) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => return Ok(()),
    };

    // we try updating our local host ids cache first intentionally
    {
        let mut ids = ctx.data().guild_configs.lock().unwrap();
        let entry = ids.entry(guild_id.0).or_default();
        entry.host_id = Some(id);
    }

    sqlx::query!(
        "INSERT INTO config (guild_id, host_role_id) VALUES ($1, $2)
        ON CONFLICT (guild_id) DO UPDATE SET host_role_id = EXCLUDED.host_role_id;",
        guild_id.0 as i64,
        id as i64
    )
    .execute(&ctx.data().db_pool)
    .await?;

    ctx.say("Set host role id.").await?;

    Ok(())
}

/// Sets the fnf self roles channel ID.
#[poise::command(prefix_command, owners_only, guild_only)]
pub async fn fnfchannel(
    ctx: Context<'_>,
    #[description = "The fnf channel id"] channel_id: serenity::ChannelId,
) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => return Ok(()),
    };

    // we try updating our local host ids cache first intentionally
    {
        let mut ids = ctx.data().guild_configs.lock().unwrap();
        let entry = ids.entry(guild_id.0).or_default();
        entry.channel_id = Some(channel_id.0);
    }

    sqlx::query!(
        "INSERT INTO config (guild_id, fnf_channel_id) VALUES ($1, $2)
        ON CONFLICT (guild_id) DO UPDATE SET fnf_channel_id = EXCLUDED.fnf_channel_id;",
        guild_id.0 as i64,
        channel_id.0 as i64
    )
    .execute(&ctx.data().db_pool)
    .await?;

    ctx.say(format!("Set {} as the fnf channel.", channel_id.mention()))
        .await?;

    Ok(())
}

/// Shuts down the bot.
#[poise::command(prefix_command, owners_only)]
pub async fn shutdown(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Shutting down... 👋").await?;
    ctx.framework()
        .shard_manager()
        .lock()
        .await
        .shutdown_all()
        .await;
    Ok(())
}

/// Sends the bot's invite URL.
#[poise::command(prefix_command, owners_only)]
pub async fn invite(ctx: Context<'_>) -> Result<(), Error> {
    let user = if let Some(cache) = ctx.cache() {
        cache.current_user()
    } else {
        error!("unable to get cache");
        return Err("unable to get cache".into());
    };
    ctx.say(format!(
        "You can invite the bot using the following URL: {}",
        invite_url(&user, &ctx).await?,
    ))
    .await?;
    Ok(())
}

/// Removes a room from the database.
#[poise::command(prefix_command, check = "is_host")]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Message ID for the room"] message_id: u64,
) -> Result<(), Error> {
    sqlx::query!(
        "DELETE FROM message WHERE message_id = $1",
        message_id as i64
    )
    .execute(&ctx.data().db_pool)
    .await?;

    // only remove from our local cache if database removal is successful
    {
        ctx.data().messages.lock().unwrap().remove(&message_id);
    }

    ctx.say(format!(
        "Removed the room associated with message {}",
        message_id
    ))
    .await?;

    Ok(())
}

/// Returns true if user is a host
async fn is_host(ctx: Context<'_>) -> Result<bool, Error> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => return Ok(false),
    };

    let host_role_id = {
        if let Some(GuildConfig {
            channel_id: _,
            host_id,
        }) = ctx.data().guild_configs.lock().unwrap().get(&guild_id.0)
        {
            if let Some(id) = host_id {
                *id
            } else {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    };

    Ok(ctx.author().has_role(&ctx, guild_id, host_role_id).await?)
}

fn format_user_str(uid: i64) -> String {
    format!("<@{uid}> ({uid})\n")
}
