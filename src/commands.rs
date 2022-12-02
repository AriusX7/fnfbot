use poise::serenity_prelude::{self as serenity, CacheHttp, Mentionable};
use tracing::error;

use crate::{invite_url, Context, Error, EMBED_COLOUR, REACT_STR};

/// Set up self-role reaction message for a new room.
#[poise::command(prefix_command, guild_only, check = "is_host")]
pub async fn host(
    ctx: Context<'_>,
    #[description = "Channel for the room"] channel: serenity::GuildChannel,
    #[description = "Date for the room"] date: String,
    #[description = "Time for the room"]
    #[rest]
    time: String,
) -> Result<(), Error> {
    let exit = {
        let channels = ctx.data().channels_and_messages.lock().unwrap();
        channels.contains_key(&channel.id.0)
    };

    if exit {
        ctx.say("Self-role message already exists for this channel.")
            .await?;
        return Ok(());
    }

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
        "INSERT INTO channel_message VALUES($1, $2) ON CONFLICT (channel_id) DO NOTHING",
        channel.id.0 as i64,
        msg.id.0 as i64,
    )
    .execute(&ctx.data().db_pool)
    .await?;

    {
        let mut channels = ctx.data().channels_and_messages.lock().unwrap();
        channels.insert(channel.id.0, msg.id.0);
    }

    ctx.say("Self-role reaction message was set up successfully.")
        .await?;

    Ok(())
}

/// Shows the users that signed up for the room.
#[poise::command(prefix_command, guild_only, check = "is_host")]
pub async fn reacts(
    ctx: Context<'_>,
    #[description = "Channel for the room"]
    #[rest]
    channel: serenity::GuildChannel,
) -> Result<(), Error> {
    let records = sqlx::query!(
        "SELECT user_id FROM react WHERE channel_id = $1 ORDER BY react_num",
        channel.id.0 as i64
    )
    .fetch_all(&ctx.data().db_pool)
    .await?;

    let mut embed = serenity::CreateEmbed::default();
    embed.colour(EMBED_COLOUR);

    let desc_start = format!("**Signups for Room {}**", channel.mention());
    if records.is_empty() {
        embed.description(format!("{desc_start}\n\nNo signups yet."));
    } else {
        embed.description(desc_start);
        let mut in_room = String::new();
        let mut reserves = String::new();

        for (i, record) in records.iter().enumerate() {
            if i < 9 {
                in_room.push_str(&format_user_str(record.user_id));
            } else {
                reserves.push_str(&format_user_str(record.user_id));
            }
        }

        embed.field(
            format!("In Room ({}/9)", records.len().min(9)),
            in_room,
            true,
        );

        if records.len() >= 9 {
            embed.field(
                format!("Reserves ({}/5)", records.len() - 9),
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
        let mut ids = ctx.data().host_ids.lock().unwrap();
        ids.insert(guild_id.0, Some(id));
    }

    sqlx::query!(
        "INSERT INTO config VALUES ($1, $2)
        ON CONFLICT (guild_id) DO UPDATE SET host_role_id = EXCLUDED.host_role_id;",
        guild_id.0 as i64,
        id as i64
    )
    .execute(&ctx.data().db_pool)
    .await?;

    ctx.say("Set host role id.").await?;

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

/// Returns true if user is a host
async fn is_host(ctx: Context<'_>) -> Result<bool, Error> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => return Ok(false),
    };

    let host_role_id = {
        if let Some(Some(id)) = ctx.data().host_ids.lock().unwrap().get(&guild_id.0) {
            *id
        } else {
            return Ok(false);
        }
    };

    Ok(ctx.author().has_role(&ctx, guild_id, host_role_id).await?)
}

fn format_user_str(uid: i64) -> String {
    format!("<@{uid}> ({uid})\n")
}
