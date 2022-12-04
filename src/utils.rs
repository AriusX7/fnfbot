use poise::serenity_prelude::{
    async_trait,
    parse_message_id_pair,
    parse_message_url,
    ArgumentConvert,
    ChannelId,
    Context,
    GuildId,
    MessageId,
    MessageParseError,
};

use crate::Data;

pub struct ParseableMessageId(pub u64);

#[async_trait]
impl ArgumentConvert for ParseableMessageId {
    type Err = MessageParseError;

    async fn convert(
        _ctx: &Context,
        _guild_id: Option<GuildId>,
        _channel_id: Option<ChannelId>,
        s: &str,
    ) -> Result<Self, Self::Err> {
        let extract_from_message_id = || Some(MessageId(s.parse().ok()?));

        let extract_from_message_url = || {
            let (_guild_id, _channel_id, message_id) =
                parse_message_url(&s.replace("canary.", ""))?;
            Some(message_id)
        };

        parse_message_id_pair(s)
            .map(|(_, m)| m)
            .or_else(extract_from_message_id)
            .or_else(extract_from_message_url)
            .ok_or(MessageParseError::Malformed)
            .map(|m| ParseableMessageId(m.0))
    }
}

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
