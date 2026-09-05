//! Session caches (chats, handles, contacts, tapbacks).

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
};

use imessage_database::{
    tables::{
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::Handle,
        messages::Message,
        table::{Cacheable, ME, UNKNOWN},
    },
    util::dates::get_offset,
};

use crate::{contacts::Name, data_source::DataSource, error::RuntimeError, options::MailOptions};

/// Setup steps `MailSession::new` runs before any message is read.
const CACHE_STEPS: usize = 4;

/// Cached chats, handles, contacts, and tapbacks for one conversion run.
pub(crate) struct MailSession {
    pub options: MailOptions,
    pub offset: i64,
    pub data_source: DataSource,
    pub chatrooms: HashMap<i32, Chat>,
    pub real_chatrooms: HashMap<i32, i32>,
    pub chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    pub participants: HashMap<i32, Name>,
    pub real_participants: HashMap<i32, i32>,
    /// Tapbacks keyed by target message GUID → part index → reactions.
    pub tapbacks: HashMap<String, HashMap<usize, Vec<Message>>>,
    /// Chat IDs already reported as having no handle rows (avoids log spam).
    logged_handleless_chats: RefCell<HashSet<i32>>,
}

impl MailSession {
    /// Load chats, handles, contacts, and tapbacks from the Messages database.
    ///
    /// # Errors
    ///
    /// Returns an error when the data source cannot be opened or a cache query
    /// fails.
    pub fn new(options: MailOptions) -> Result<Self, RuntimeError> {
        let data_source = DataSource::from(&options)?;

        options.emit_log("Building cache...");
        options.setup_step(1, CACHE_STEPS, "Caching chats");
        let chatrooms = Chat::cache(data_source.db())?;

        options.setup_step(2, CACHE_STEPS, "Caching chatrooms");
        let chatroom_participants = ChatToHandle::cache(data_source.db())?;
        let chat_handle_lookup = ChatToHandle::get_chat_lookup_map(data_source.db())?;
        let real_chatrooms = ChatToHandle::dedupe(&chatroom_participants, &chat_handle_lookup)?;

        options.setup_step(3, CACHE_STEPS, "Caching participants");
        let participants = Handle::cache(data_source.db())?;
        let real_participants = Handle::dedupe(&participants);
        let participants_map = data_source
            .contacts_index
            .build_participants_map(&participants, &real_participants);

        options.setup_step(4, CACHE_STEPS, "Caching tapbacks");
        let tapbacks = Message::cache(data_source.db())?;
        options.emit_log("Cache built!");

        Ok(Self {
            chatrooms,
            real_chatrooms,
            chatroom_participants,
            real_participants,
            participants: participants_map,
            tapbacks,
            logged_handleless_chats: RefCell::new(HashSet::new()),
            options,
            offset: get_offset(),
            data_source,
        })
    }

    /// Chat row and deduped chat id for a message, if the chat has participants.
    pub fn conversation(&self, message: &Message) -> Option<(&Chat, &i32)> {
        match message.chat_id.or(message.deleted_from) {
            Some(chat_id) => {
                if let Some(chatroom) = self.chatrooms.get(&chat_id) {
                    match self.real_chatrooms.get(&chat_id) {
                        Some(real_id) => Some((chatroom, real_id)),
                        // Chat row exists but has no handle rows: no chat
                        // context is available, so messages land in ORPHANED.
                        // Report it once per chat so users can see why.
                        None => {
                            if self.logged_handleless_chats.borrow_mut().insert(chat_id) {
                                self.options.emit_log(format!(
                                    "Chat ID {chat_id} has no participant handles; \
                                     its messages will be exported under ORPHANED"
                                ));
                            }
                            None
                        }
                    }
                } else {
                    self.options
                        .emit_log(format!("Chat ID {chat_id} does not exist in chat table!"));
                    None
                }
            }
            None => None,
        }
    }

    /// Display name for the sender: Me / caller id, a contact name, or Unknown.
    pub fn who<'a, 'b: 'a>(
        &'a self,
        handle_id: Option<i32>,
        is_from_me: bool,
        destination_caller_id: &'b Option<String>,
    ) -> &'a str {
        if is_from_me {
            if self.options.use_caller_id {
                return destination_caller_id.as_deref().unwrap_or(ME);
            }
            return ME;
        } else if let Some(handle_id) = handle_id {
            return match self.resolve_participant(handle_id) {
                Some(contact) => contact.get_display_name(),
                None => UNKNOWN,
            };
        }
        UNKNOWN
    }

    /// Contact name for a handle id after merging duplicate handles.
    pub fn resolve_participant(&self, handle_id: i32) -> Option<&Name> {
        self.real_participants
            .get(&handle_id)
            .and_then(|internal_id| self.participants.get(internal_id))
    }
}
