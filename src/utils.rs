use std::time::Duration;

use poise::serenity_prelude::{
    parse_message_id_pair,
    parse_message_url,
    ChannelId,
    GuildId,
    MessageId,
    MessageParseError,
};
use sqlx::PgPool;

use crate::{Context, Data, Error};

pub fn get_message_link(message_id: u64, data: &Data, guild_id: GuildId) -> Option<String> {
    data.guild_configs
        .get(&guild_id.0)
        .map(|c| c.channel_id)
        .unwrap_or_default()
        .map(|i| MessageId(message_id).link(ChannelId(i), Some(guild_id)))
}

pub async fn get_message_id(input: &str, pool: &PgPool) -> Result<MessageId, Error> {
    if let Ok(i) = input.parse::<i32>() {
        if let Some(res) = sqlx::query!("SELECT message_id FROM message WHERE num = $1", i as i32)
            .fetch_optional(pool)
            .await?
        {
            return Ok(MessageId(res.message_id as u64));
        }
    }

    let extract_from_message_id = || Some(MessageId(input.parse().ok()?));
    let extract_from_message_url = || Some(parse_message_url(&input.replace("canary.", ""))?.2);

    parse_message_id_pair(input)
        .map(|(_, m)| m)
        .or_else(extract_from_message_id)
        .or_else(extract_from_message_url)
        .ok_or_else(|| MessageParseError::Malformed.into())
}

pub async fn confirm_prompt(ctx: &Context<'_>, timeout: f32, answer: &str) -> bool {
    matches!(
        ctx.author()
            .await_reply(ctx)
            .channel_id(ctx.channel_id())
            .timeout(Duration::from_secs_f32(timeout))
            .await,
        Some(m) if m.content == answer,
    )
}
