# nexus-chat SpacetimeDB Module

Real-time backend for the Nexus Terminal CHAT service. Discord-parity feature set with 40 tables, 95 reducers, and 4 scheduled job types.

**Module name:** `nexus-chat`
**Host:** `wss://maincloud.spacetimedb.com`
**SpacetimeDB version:** 2.0
**Rust toolchain:** 1.93.0 (required)

---

## Directory Structure

```
spacetimedb-module/
├── Cargo.toml                    # Dependencies (spacetimedb 2.0, serde, serde_json, log)
├── README.md                     # This file
└── src/
    ├── lib.rs                    # Module root: mod declarations + timestamp_ms()
    ├── tables/
    │   ├── mod.rs                # Table module declarations + re-exports
    │   ├── auth.rs               # UserIdentityLink (multi-device identity mapping)
    │   ├── users.rs              # ChatUser (profiles, status, tier)
    │   ├── rooms.rs              # Room, RoomMember, RoomInvitation, ReadPosition, Draft, TypingIndicator
    │   ├── messages.rs           # Message, MessageEdit, Reaction, ScheduledMessage + 2 scheduled job tables
    │   ├── servers.rs            # ChatServer (guilds)
    │   ├── server_members.rs     # ServerMember, ChannelCategory, ServerRole, MemberRole, ChannelOverride
    │   └── discord_parity.rs     # ServerEmoji, Sticker, MessageAttachment, PinnedMessage, Poll, PollVote,
    │                             # ServerInvite, AuditLog, UserProfile, NotificationSetting, MemberTimeout,
    │                             # AutoModRule, Bookmark, ForumTag, ForumPostTag, Webhook, WebhookMessage,
    │                             # TimeoutExpiryJob
    ├── reducers/
    │   ├── mod.rs                # Reducer module declarations + re-exports
    │   ├── lifecycle.rs          # init, client_connected, client_disconnected
    │   ├── users.rs              # register_identity, set_display_name, set_status, sync_user_tier
    │   ├── rooms.rs              # create_room, join_room, leave_room
    │   ├── messages.rs           # send_message, send_ephemeral_message, edit_message, start_typing,
    │   │                         # expire_typing_indicator, toggle_reaction, mark_message_read,
    │   │                         # mark_room_read, schedule_message, cancel_scheduled_message,
    │   │                         # send_scheduled_message, delete_ephemeral_message, kick_user,
    │   │                         # ban_user, promote_to_admin
    │   ├── private_rooms.rs      # invite_to_room, respond_to_invitation, start_dm, update_draft, clear_draft
    │   ├── servers.rs            # create_server, update_server, delete_server, join_server, leave_server,
    │   │                         # kick_from_server, ban_from_server, set_server_member_role,
    │   │                         # create_category, update_category, delete_category, move_room_to_category,
    │   │                         # create_default_role, create_role, update_role, delete_role,
    │   │                         # assign_role, remove_role, set_channel_override, delete_channel_override
    │   └── discord_parity.rs     # create_server_emoji, delete_server_emoji, rename_server_emoji,
    │                             # create_sticker, delete_sticker, add_attachment, delete_attachment,
    │                             # pin_message, unpin_message, create_poll, vote_poll, unvote_poll,
    │                             # close_poll, create_server_invite, delete_server_invite, use_server_invite,
    │                             # log_audit_event, update_profile, set_avatar, set_custom_status,
    │                             # set_notification_level, mute_channel, unmute_channel,
    │                             # timeout_member, remove_timeout, expire_member_timeout,
    │                             # create_auto_mod_rule, update_auto_mod_rule, delete_auto_mod_rule,
    │                             # add_bookmark, remove_bookmark, update_bookmark_note,
    │                             # create_forum_post, add_forum_tag, remove_forum_tag,
    │                             # tag_forum_post, untag_forum_post,
    │                             # create_thread, archive_thread, unarchive_thread, lock_thread, unlock_thread,
    │                             # create_webhook, delete_webhook, send_webhook_message
    └── utils/
        ├── mod.rs                # Utility module declarations
        ├── auth.rs               # sender_hex, get_caller_user_id, is_system_caller, require_server_admin
        ├── time.rs               # timestamp_ms() helper
        └── validation.rs         # Input validation helpers

generated/
└── (auto-populated)              # TypeScript types from spacetime generate

services/
├── ChatSpacetimeDBService.ts     # Connection + 87 reducer wrappers + subscriptions
├── spacetimedb-subscriptions.ts  # Table accessor map + subscription helpers
└── spacetimedb-index.ts          # Barrel re-exports

hooks/
└── useChatSpacetimeDB.ts         # React hook wrapping ChatProvider context
```

---

## Tables (40 total)

### Authentication (1 table)

| Table | Public | Purpose |
|-------|--------|---------|
| `user_identity_links` | Yes | Maps SpacetimeDB identity (hex) to platform user_id. PK: `stdb_identity` with btree index. One user can have many identities (multi-device). |

### Users (1 table)

| Table | Public | Purpose |
|-------|--------|---------|
| `chat_users` | Yes | User profiles: display_name, status, online flag, tier, avatar, custom_status. PK: `user_id`. |

### Rooms & Channels (6 tables)

| Table | Public | Purpose |
|-------|--------|---------|
| `rooms` | Yes | Channel definitions. Supports 5 types: text, forum, announcement, rules, voice. Fields: server_id, category_id, topic, slowmode, nsfw, archived, locked. |
| `room_members` | Yes | Room membership with role (member/admin/owner). Indexed by room_id, user_id. |
| `room_invitations` | Yes | Invites to private rooms. Status: pending/accepted/declined. |
| `read_positions` | Yes | Per-user, per-room read tracking (last_read_message_id). |
| `drafts` | Yes | Per-room message drafts synced to server. |
| `typing_indicators` | Yes | Auto-expiring typing status. Cleaned by scheduled job. |

### Messages (4 tables + 2 scheduled)

| Table | Public | Purpose |
|-------|--------|---------|
| `messages` | Yes | Message content with threading (parent_message_id, reply_to_id), ephemeral flag, mentions, flags bitfield. |
| `message_edits` | Yes | Full edit history: old_content, new_content, editor, timestamp. |
| `reactions` | Yes | Emoji reactions per message. Toggle semantics (add if absent, remove if present). |
| `scheduled_messages` | Yes | Messages queued for future delivery with scheduled job reference. |
| `typing_expiry_jobs` | Sched | Scheduled: auto-remove typing indicators. |
| `ephemeral_cleanup_jobs` | Sched | Scheduled: auto-delete ephemeral messages. |

### Servers (5 tables)

| Table | Public | Purpose |
|-------|--------|---------|
| `chat_servers` | Yes | Server (guild) definitions. Fields: name, description, audience_id, owner, is_public, default_tier, icon_url. |
| `server_members` | Yes | Server membership with role, nickname, timeout_until, deaf/mute. Composite PK: `{server_id}-{user_id}`. |
| `channel_categories` | Yes | Channel grouping with sort_order for sidebar display. |
| `server_roles` | Yes | Role definitions: name, color, permissions (u64 bitfield), mentionable, is_default. |
| `member_roles` | Yes | Many-to-many role assignments. Composite PK: `{server_id}-{user_id}-{role_id}`. |

### Discord Parity (19 tables + 1 private + 2 scheduled)

| Table | Public | Purpose |
|-------|--------|---------|
| `channel_overrides` | Yes | Per-channel permission overrides (allow/deny bitfields) for roles or users. |
| `server_emojis` | Yes | Custom emoji per server: name, image_data (base64), animated flag. |
| `stickers` | Yes | Custom stickers per server: name, description, image_data, tags. |
| `message_attachments` | Yes | File attachments: filename, URL, size, content_type, dimensions, spoiler flag. |
| `pinned_messages` | Yes | Pinned messages per room: message_id, pinned_by, pinned_at. |
| `polls` | Yes | Polls attached to messages: question, options (JSON), allow_multiple, anonymous, expires_at, closed. |
| `poll_votes` | Yes | Individual votes. Composite PK: `{poll_id}-{user_id}-{option_index}`. |
| `server_invites` | Yes | Invite codes: max_uses, uses, expires_at. PK is short alphanumeric code. |
| `audit_log` | Yes | Server action log: action, actor, target_type, target_id, details (JSON). |
| `user_profiles` | Yes | Extended profiles: about_me, banner_color/data, pronouns, accent_color. |
| `notification_settings` | Yes | Per-target notification level, suppress flags, mute_until. |
| `member_timeouts` | Yes | Temporary timeouts: reason, expires_at, issued_by. |
| `auto_mod_rules` | Yes | Moderation rules: type, config (JSON), action, exempt_roles/channels (JSON). |
| `bookmarks` | Yes | Personal message bookmarks with optional note. |
| `forum_tags` | Yes | Tags for forum channels: name, emoji, color, sort_order. |
| `forum_post_tags` | Yes | Tag assignments to forum threads. Composite PK: `{thread_room_id}-{tag_id}`. |
| `webhook_messages` | Yes | Messages sent by webhooks (public, display name/avatar). |
| `webhooks` | **No** | Webhook configs with secret tokens. **Private table** (reducer-only, never sent to clients). |
| `timeout_expiry_jobs` | Sched | Scheduled: auto-remove member timeouts. |
| `scheduled_delivery_jobs` | Sched | Scheduled: send messages at scheduled time. |

---

## Reducers (95 total)

### Lifecycle (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `init` | System | Creates #general room on module init |
| `client_connected` | Auto | Sets user online, updates identity mapping, registers last_seen |
| `client_disconnected` | Auto | Sets user offline, cleans up typing indicators |

### Authentication (1)

| Reducer | Auth | Description |
|---------|------|-------------|
| `register_identity` | Any | Links SpacetimeDB identity to platform user_id |

### Users (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `set_display_name` | User | Update display name |
| `set_status` | User | Set presence status (online/away/dnd/offline) |
| `sync_user_tier` | User | Sync tier from platform (free/pro/creator/team) |

### Rooms (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_room` | User | Create room with name, privacy, server, tier, description, sort_order |
| `join_room` | User | Join a public room |
| `leave_room` | User | Leave a room |

### Messages (7)

| Reducer | Auth | Description |
|---------|------|-------------|
| `send_message` | User | Send message with optional parent_message_id (threading) |
| `send_ephemeral_message` | User | Send auto-deleting message with TTL |
| `edit_message` | Owner | Edit own message (creates edit history record) |
| `start_typing` | User | Signal typing (creates indicator, schedules expiry) |
| `expire_typing_indicator` | Scheduled | Auto-remove expired typing indicator |
| `toggle_reaction` | User | Add or remove emoji reaction (toggle semantics) |
| `delete_ephemeral_message` | Scheduled | Delete expired ephemeral message |

### Read Tracking (2)

| Reducer | Auth | Description |
|---------|------|-------------|
| `mark_message_read` | User | Update read position to specific message |
| `mark_room_read` | User | Mark all messages in room as read |

### Scheduling (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `schedule_message` | User | Queue message for future delivery |
| `cancel_scheduled_message` | Owner | Cancel pending scheduled message |
| `send_scheduled_message` | Scheduled | Deliver scheduled message at send_at time |

### Room Moderation (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `kick_user` | Room Admin | Remove user from room |
| `ban_user` | Room Admin | Ban user from room |
| `promote_to_admin` | Room Admin | Grant admin role in room |

### Private Rooms & DMs (5)

| Reducer | Auth | Description |
|---------|------|-------------|
| `invite_to_room` | Room Member | Invite user to private room |
| `respond_to_invitation` | Invitee | Accept or decline room invitation |
| `start_dm` | User | Create private DM room with target user |
| `update_draft` | User | Save draft for room |
| `clear_draft` | User | Clear draft for room |

### Servers (8)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_server` | User | Create server with name, audience, public flag, default tier |
| `update_server` | Owner/Admin | Update server properties |
| `delete_server` | Owner | Delete server and all related data |
| `join_server` | User | Join public server |
| `leave_server` | Member | Leave server |
| `kick_from_server` | Admin | Kick member from server |
| `ban_from_server` | Admin | Ban member from server |
| `set_server_member_role` | Owner | Set member's server role |

### Categories (4)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_category` | Admin | Create channel category with sort_order |
| `update_category` | Admin | Rename or reorder category |
| `delete_category` | Admin | Delete category (ungroups channels) |
| `move_room_to_category` | Admin | Assign room to a category |

### Roles (6)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_default_role` | Internal | Create @everyone role for server |
| `create_role` | Admin | Create role with name, color, permissions, mentionable |
| `update_role` | Admin | Update role properties |
| `delete_role` | Admin | Delete role |
| `assign_role` | Admin | Assign role to member |
| `remove_role` | Admin | Remove role from member |

### Channel Overrides (2)

| Reducer | Auth | Description |
|---------|------|-------------|
| `set_channel_override` | Admin | Set allow/deny permission override for role or user |
| `delete_channel_override` | Admin | Remove channel override |

### Emojis & Stickers (5)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_server_emoji` | Admin | Upload custom emoji (name, image_data, animated) |
| `delete_server_emoji` | Admin | Delete custom emoji |
| `rename_server_emoji` | Admin | Rename custom emoji |
| `create_sticker` | Admin | Upload sticker (name, description, image_data, tags) |
| `delete_sticker` | Admin | Delete sticker |

### Attachments (2)

| Reducer | Auth | Description |
|---------|------|-------------|
| `add_attachment` | User | Attach file to message (name, URL, size, type, dimensions, spoiler) |
| `delete_attachment` | Owner | Delete own attachment |

### Pins (2)

| Reducer | Auth | Description |
|---------|------|-------------|
| `pin_message` | Admin | Pin message to channel |
| `unpin_message` | Admin | Unpin message from channel |

### Polls (4)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_poll` | User | Create poll with question, options (JSON), allow_multiple, anonymous |
| `vote_poll` | User | Vote on poll option |
| `unvote_poll` | User | Remove vote from poll option |
| `close_poll` | Creator | Close poll early |

### Server Invites (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_server_invite` | Admin | Create invite code with max_uses, expires_at |
| `delete_server_invite` | Admin | Delete invite code |
| `use_server_invite` | User | Use invite code to join server |

### Audit Log (1)

| Reducer | Auth | Description |
|---------|------|-------------|
| `log_audit_event` | Internal | Record audit log entry (called by other reducers) |

### Profiles (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `update_profile` | User | Update about_me, banner, pronouns, accent_color |
| `set_avatar` | User | Set avatar image (base64 data) |
| `set_custom_status` | User | Set custom status text and emoji |

### Notifications (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `set_notification_level` | User | Set notification level for channel or server |
| `mute_channel` | User | Mute channel for duration |
| `unmute_channel` | User | Unmute channel |

### Timeouts (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `timeout_member` | Admin | Timeout member for duration with reason |
| `remove_timeout` | Admin | Remove timeout early |
| `expire_member_timeout` | Scheduled | Auto-remove expired timeout |

### Auto-Mod (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_auto_mod_rule` | Admin | Create rule (type, config, action, exemptions) |
| `update_auto_mod_rule` | Admin | Update rule config/action/enabled/exemptions |
| `delete_auto_mod_rule` | Admin | Delete rule |

### Bookmarks (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `add_bookmark` | User | Bookmark message with optional note |
| `remove_bookmark` | Owner | Remove own bookmark |
| `update_bookmark_note` | Owner | Edit bookmark note |

### Forums (5)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_forum_post` | User | Create forum thread (creates child room + first message) |
| `add_forum_tag` | Admin | Add tag to forum channel |
| `remove_forum_tag` | Admin | Remove forum tag |
| `tag_forum_post` | User | Apply tag to forum post |
| `untag_forum_post` | User | Remove tag from forum post |

### Threads (5)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_thread` | User | Create thread from message (creates child room) |
| `archive_thread` | Admin | Archive thread (set archived flag) |
| `unarchive_thread` | Admin | Unarchive thread |
| `lock_thread` | Admin | Lock thread (prevent new messages) |
| `unlock_thread` | Admin | Unlock thread |

### Webhooks (3)

| Reducer | Auth | Description |
|---------|------|-------------|
| `create_webhook` | Admin | Create webhook for channel (name, avatar) |
| `delete_webhook` | Admin | Delete webhook |
| `send_webhook_message` | Token | Send message via webhook (token auth, not user auth) |

---

## Permission Bitfield

Server roles use a `u64` permission bitfield:

| Bit | Constant | Description |
|-----|----------|-------------|
| 0 | `SEND_MESSAGES` | Can send messages in channels |
| 1 | `MANAGE_CHANNELS` | Can create/edit/delete channels |
| 2 | `KICK_MEMBERS` | Can kick members from server |
| 3 | `BAN_MEMBERS` | Can ban members from server |
| 4 | `MANAGE_ROLES` | Can create/edit/delete roles |
| 5 | `PIN_MESSAGES` | Can pin/unpin messages |
| 6 | `MANAGE_EMOJIS` | Can upload/delete custom emojis |
| 7 | `MANAGE_WEBHOOKS` | Can create/delete webhooks |
| 31 | `ADMINISTRATOR` | Full permissions (bypasses all checks) |

Channel overrides use `allow`/`deny` bitfields with the same bit positions.

---

## Scheduled Jobs

| Job Table | Trigger Reducer | Auto-Scheduled By |
|-----------|----------------|-------------------|
| `typing_expiry_jobs` | `expire_typing_indicator` | `start_typing` (5s delay) |
| `ephemeral_cleanup_jobs` | `delete_ephemeral_message` | `send_ephemeral_message` (TTL delay) |
| `scheduled_delivery_jobs` | `send_scheduled_message` | `schedule_message` (user-set time) |
| `timeout_expiry_jobs` | `expire_member_timeout` | `timeout_member` (duration delay) |

---

## Security Model

### Identity Resolution
Every write reducer calls `get_caller_user_id(ctx)` to resolve `ctx.sender` (SpacetimeDB hex identity) through `user_identity_links` to the platform `user_id`. This ensures:
- User_id is always server-derived, never accepted from client params
- Multi-device access (one user, many identities) works transparently

### Auth Guard Levels

| Level | Description | Used By |
|-------|-------------|---------|
| User | Any authenticated user with identity link | send_message, join_room, vote_poll, etc. |
| Owner | Caller must own the resource | edit_message, cancel_scheduled_message, delete_server |
| Admin | Caller must be server admin/owner | kick, ban, create_role, pin_message, etc. |
| Room Admin | Caller must be room admin/owner | kick_user, ban_user, promote_to_admin |
| System | Scheduled/internal only (`ctx.sender == ctx.identity()`) | expire_*, log_audit_event |
| Token | Webhook token authentication | send_webhook_message |

### Private Tables
Only `webhooks` is private (contains secret tokens). All other tables are public with access controlled by:
1. Client-side subscription scoping (WHERE user_id = ...)
2. Reducer auth guards (server-side)

---

## Deploy Commands

```bash
# Full deploy: generate TypeScript types + publish module (preserves data)
bash scripts/spacetimedb-publish-chat.sh

# Generate TypeScript types only (no publish)
bash scripts/spacetimedb-publish-chat.sh generate

# Publish only (skip type generation)
bash scripts/spacetimedb-publish-chat.sh publish

# DESTRUCTIVE: Deploy with data wipe (only for breaking schema changes)
bash scripts/spacetimedb-publish-chat.sh --clear
```

### Cargo Check (Development)

```bash
export PATH="/home/runner/workspace/.cargo/bin:/usr/bin:/bin:$PATH"
rustup run 1.93.0 cargo check --manifest-path commands/standalone/CHAT/spacetimedb-module/Cargo.toml
```

**NEVER run bare `cargo check`** — Rust 1.92+ interacts badly with some Linux runtime loaders (notably Replit's `LD_AUDIT`), producing `cannot allocate memory in static TLS block`. Always use `rustup run 1.93.0` and set `GLIBC_TUNABLES=glibc.rtld.optional_static_tls=65536` if you hit the error.

### View Logs

```bash
export PATH="/home/runner/workspace/.local/bin:$PATH"
spacetime logs nexus-chat -f
```

---

## Schema Change Rules

| Rule | Detail |
|------|--------|
| New fields MUST be `Option<T>` | Never add required fields to existing tables |
| Append only | New fields go at END of struct |
| Never remove columns | Mark as deprecated, remove in future version |
| Never change column types | Add new column with new type, migrate data |
| Never use `--clear` without permission | It deletes ALL data — stop and ask first |

### Migration Compatibility

| Change | Data Preserved? |
|--------|----------------|
| Add new table | Yes |
| Add `Option<T>` column at end | Yes |
| Add/remove reducers, indexes | Yes |
| Remove table or column | **No** |
| Change column type | **No** |
| Add required (non-Option) column | **No** |

---

## TypeScript Client Integration

### Subscription Scopes

**Core** (on authenticated connect):
```sql
SELECT * FROM chat_users
SELECT * FROM rooms
SELECT * FROM chat_servers
SELECT * FROM user_profiles
SELECT * FROM server_members WHERE user_id = '{userId}'
SELECT * FROM room_members WHERE user_id = '{userId}'
SELECT * FROM room_invitations WHERE invitee_id = '{userId}'
SELECT * FROM drafts WHERE user_id = '{userId}'
SELECT * FROM read_positions WHERE user_id = '{userId}'
SELECT * FROM scheduled_messages WHERE author_id = '{userId}'
SELECT * FROM notification_settings WHERE user_id = '{userId}'
SELECT * FROM bookmarks WHERE user_id = '{userId}'
```

**Room** (on room select):
```sql
SELECT * FROM messages WHERE room_id = '{roomId}'
SELECT * FROM room_members WHERE room_id = '{roomId}'
SELECT * FROM typing_indicators WHERE room_id = '{roomId}'
SELECT * FROM reactions WHERE room_id = '{roomId}'
SELECT * FROM message_edits WHERE room_id = '{roomId}'
SELECT * FROM scheduled_messages WHERE room_id = '{roomId}'
SELECT * FROM pinned_messages WHERE room_id = '{roomId}'
SELECT * FROM message_attachments WHERE room_id = '{roomId}'
SELECT * FROM polls WHERE room_id = '{roomId}'
SELECT * FROM poll_votes WHERE room_id = '{roomId}'
SELECT * FROM webhook_messages WHERE room_id = '{roomId}'
SELECT * FROM channel_overrides WHERE room_id = '{roomId}'
SELECT * FROM forum_tags WHERE room_id = '{roomId}'
SELECT * FROM forum_post_tags WHERE thread_room_id = '{roomId}'
```

**Server** (on server select):
```sql
SELECT * FROM channel_categories WHERE server_id = '{serverId}'
SELECT * FROM server_roles WHERE server_id = '{serverId}'
SELECT * FROM member_roles WHERE server_id = '{serverId}'
SELECT * FROM server_emojis WHERE server_id = '{serverId}'
SELECT * FROM stickers WHERE server_id = '{serverId}'
SELECT * FROM server_invites WHERE server_id = '{serverId}'
SELECT * FROM auto_mod_rules WHERE server_id = '{serverId}'
SELECT * FROM member_timeouts WHERE server_id = '{serverId}'
SELECT * FROM server_members WHERE server_id = '{serverId}'
SELECT * FROM audit_log WHERE server_id = '{serverId}'
```

**Public** (guest mode):
```sql
SELECT * FROM chat_servers
SELECT * FROM rooms
SELECT * FROM chat_users
SELECT * FROM user_profiles
```

### SDK Patterns

```typescript
// Object arguments (v1.11+ style)
service.reducers.sendMessage({ id, roomId, content, parentMessageId: undefined });

// bigint for u64 fields
service.reducers.scheduleMessage({ id, roomId, content, sendAt: BigInt(Date.now()) * 1000n });

// Option fields use undefined (not null)
service.reducers.createRoom({ id, name, isPrivate: false, serverId: serverId ?? undefined });
```

---

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `cannot allocate memory in static TLS block` | Using `stable` toolchain | Use `rustup run 1.93.0 cargo check` |
| `no method named 'insert' found` | Missing `Table` trait | Add `use spacetimedb::Table;` |
| `no method named 'my_table' found for struct 'Local'` | Missing accessor import | Add `use crate::tables::file::table_name;` |
| `Expected 1 arguments, but got N` | v1.11 uses object args | Change `reducer(a, b)` to `reducer({ a, b })` |
| `Type 'null' is not assignable` | Option = `T \| undefined` | Use `value ?? undefined` |
| Publish fails: "incompatible schema" | Breaking schema change | Make backwards-compatible or `--clear` with permission |
| `ChatSpacetimeDBService not initialized` | Service used before connect | Check `service` is non-null before calling reducers |

---

## Reference Skills

Load before writing SpacetimeDB code to avoid hallucinated APIs:

| Skill | Path | When to Load |
|-------|------|--------------|
| Rust Modules | `.agents/skills/spacetimedb-rust/SKILL.md` | Writing tables, reducers, or module logic |
| TypeScript SDK | `.agents/skills/spacetimedb-typescript/SKILL.md` | Connecting from web apps, wiring services/hooks |
| Core Concepts | `.agents/skills/spacetimedb-concepts/SKILL.md` | Architecture decisions, understanding data flow |
| CLI Reference | `.agents/skills/spacetimedb-cli/SKILL.md` | Using spacetime commands (publish, sql, logs) |

---

## Resources

- [SpacetimeDB Docs](https://spacetimedb.com/docs)
- [Rust Quickstart](https://spacetimedb.com/docs/modules/rust/quickstart/)
- [TypeScript SDK](https://spacetimedb.com/docs/sdks/typescript/)
- [npm: spacetimedb](https://www.npmjs.com/package/spacetimedb)
- [GitHub](https://github.com/clockworklabs/spacetimedb)
- [Changelog](https://github.com/clockworklabs/SpacetimeDB/releases)
