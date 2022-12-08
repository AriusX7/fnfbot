use poise::serenity_prelude::{
    parse_message_id_pair,
    parse_message_url,
    ChannelId,
    GuildId,
    MessageId,
    MessageParseError,
};
use sqlx::PgPool;

use crate::{Data, Error};

pub fn get_message_link(message_id: u64, data: &Data, guild_id: GuildId) -> Option<String> {
    let channel_id = {
        if let Some(id) = data
            .guild_configs
            .lock()
            .unwrap()
            .get(&guild_id.0)
            .copied()
            .unwrap_or_default()
            .channel_id
        {
            ChannelId(id)
        } else {
            return None;
        }
    };

    Some(MessageId(message_id).link(channel_id, Some(guild_id)))
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
    let extract_from_message_url = || {
        let (_guild_id, _channel_id, message_id) =
            parse_message_url(&input.replace("canary.", ""))?;
        Some(message_id)
    };

    parse_message_id_pair(input)
        .map(|(_, m)| m)
        .or_else(extract_from_message_id)
        .or_else(extract_from_message_url)
        .ok_or_else(|| MessageParseError::Malformed.into())
}
