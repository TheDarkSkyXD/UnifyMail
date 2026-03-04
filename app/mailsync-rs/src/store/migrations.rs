// SQLite schema migrations for mailsync-rs.
//
// SQL strings are EXACT copies from the C++ constants.h file.
// Table names, column names, index names, and FTS5 tokenizer config must
// match character-for-character — the C++ sync engine reads this same database.
//
// Migration version history:
//   V1: Initial schema — all tables, indexes, FTS5 virtual tables
//   V2: MessageUIDScanIndex
//   V3: MessageBody.fetchedAt column (data-mutating migration)
//   V4: Event table additional columns
//   (V5 does not exist in C++ source — version numbers skip from 4 to 6)
//   V6: Contact table additional columns
//   V7: Label/Folder createdAt/updatedAt columns
//   V8: Thread additional columns
//   V9: ContactBook/ContactGroup/ContactContactGroup tables
//
// Current version: 9 (CURRENT_VERSION in MailStore.cpp line 103)

// ============================================================================
// V1: Initial schema — all tables, indexes, and FTS5 virtual tables
// Source: constants.h V1_SETUP_QUERIES
// ============================================================================

pub const V1_SETUP: &[&str] = &[
    // Core data tables
    "CREATE TABLE IF NOT EXISTS `_State` (id VARCHAR(40) PRIMARY KEY, value TEXT)",
    "CREATE TABLE IF NOT EXISTS `File` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), filename TEXT)",
    "CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), calendarId VARCHAR(40), _start INTEGER, _end INTEGER, is_search_indexed INTEGER DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS `Label` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data BLOB)",
    "CREATE TABLE IF NOT EXISTS `Folder` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data BLOB)",
    "CREATE TABLE IF NOT EXISTS `Thread` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data BLOB, gThrId VARCHAR(20), subject TEXT, snippet TEXT, unread INTEGER, starred INTEGER, firstMessageTimestamp INTEGER, lastMessageTimestamp INTEGER, inAllMail INTEGER, isSearchIndexed INTEGER, participants TEXT, hasAttachments INTEGER)",
    "CREATE TABLE IF NOT EXISTS `ThreadReference` (threadId VARCHAR(40), accountId VARCHAR(8), headerMessageId TEXT)",
    "CREATE TABLE IF NOT EXISTS `ThreadCategory` (id VARCHAR(40), value VARCHAR(40), inAllMail INTEGER, unread INTEGER, lastMessageReceivedTimestamp INTEGER, lastMessageSentTimestamp INTEGER)",
    "CREATE TABLE IF NOT EXISTS `ThreadCounts` (categoryId VARCHAR(40) PRIMARY KEY, unread INTEGER, total INTEGER)",
    "CREATE TABLE IF NOT EXISTS `Account` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email_address TEXT)",
    "CREATE TABLE IF NOT EXISTS `Message` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data BLOB, headerMessageId TEXT, gMsgId VARCHAR(20), gThrId VARCHAR(20), subject TEXT, date INTEGER, draft INTEGER, unread INTEGER, starred INTEGER, remoteUID INTEGER, remoteXGMLabels TEXT, remoteFolderId VARCHAR(40), replyToHeaderMessageId TEXT, threadId VARCHAR(40))",
    // ModelPluginMetadata: id = parent model id (Thread/Message), value = pluginId.
    // Multiple plugins can be attached to one model, so PK is (value, id) composite.
    // Matches C++ ModelPluginMetadata which uses pluginId as primary differentiator.
    "CREATE TABLE IF NOT EXISTS `ModelPluginMetadata` (id VARCHAR(40), accountId VARCHAR(8), objectType TEXT, value TEXT, expiration INTEGER, PRIMARY KEY (value, id))",
    "CREATE TABLE IF NOT EXISTS `DetatchedPluginMetadata` (objectId VARCHAR(40), objectType TEXT, accountId VARCHAR(8), pluginId TEXT, value TEXT, version INTEGER)",
    "CREATE TABLE IF NOT EXISTS `MessageBody` (id VARCHAR(40) PRIMARY KEY, value TEXT)",
    "CREATE TABLE IF NOT EXISTS `Contact` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email TEXT, version INTEGER)",
    "CREATE TABLE IF NOT EXISTS `Calendar` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8))",
    "CREATE TABLE IF NOT EXISTS `Task` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), status TEXT)",

    // Indexes for efficient queries (only on columns that exist in V1 Thread/Message tables)
    "CREATE INDEX IF NOT EXISTS `MessageListSortIndex` ON `Message` (accountId, threadId, date)",
    "CREATE INDEX IF NOT EXISTS `MessageDraftIndex` ON `Message` (accountId, draft, date)",
    "CREATE INDEX IF NOT EXISTS `ThreadCategoryListSortIndex` ON `ThreadCategory` (value, lastMessageReceivedTimestamp)",
    "CREATE INDEX IF NOT EXISTS `ThreadCategoryDraftSortIndex` ON `ThreadCategory` (value, lastMessageSentTimestamp)",

    // FTS5 virtual tables for full-text search
    // Note: bundled feature of rusqlite includes FTS5
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ThreadSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, subject, to_, from_, categories, body)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `EventSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, title, description, location, participants)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ContactSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, content)",
];

// ============================================================================
// V2: Message UID scan index
// Source: constants.h V2_SETUP_QUERIES
// ============================================================================

pub const V2_SETUP: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS `MessageUIDScanIndex` ON `Message` (accountId, remoteFolderId, remoteUID)",
];

// ============================================================================
// V3: MessageBody.fetchedAt column
// IMPORTANT: This migration mutates existing data — must be guarded by version check.
// Source: constants.h V3_SETUP_QUERIES
// The C++ sync engine prints "Running Migration" to stdout for this version —
// the Electron UI shows a migration progress window when it sees this string.
// ============================================================================

pub const V3_SETUP: &[&str] = &[
    "ALTER TABLE `MessageBody` ADD COLUMN fetchedAt DATETIME",
    "UPDATE `MessageBody` SET fetchedAt = datetime('now')",
];

// ============================================================================
// V4: Event table additional columns (etag, icsuid, recurrenceId, recurrenceStart, recurrenceEnd)
// Source: constants.h V4_SETUP_QUERIES
// ============================================================================

pub const V4_SETUP: &[&str] = &[
    "ALTER TABLE `Event` ADD COLUMN etag TEXT",
    "ALTER TABLE `Event` ADD COLUMN icsuid TEXT",
    "ALTER TABLE `Event` ADD COLUMN recurrenceId TEXT",
    "ALTER TABLE `Event` ADD COLUMN recurrenceStart INTEGER",
    "ALTER TABLE `Event` ADD COLUMN recurrenceEnd INTEGER",
];

// NOTE: V5 does not exist in C++ constants.h — version numbers skip from 4 to 6.

// ============================================================================
// V6: Contact table additional columns
// Source: constants.h V6_SETUP_QUERIES
// ============================================================================

pub const V6_SETUP: &[&str] = &[
    "ALTER TABLE `Contact` ADD COLUMN refs INTEGER",
    "ALTER TABLE `Contact` ADD COLUMN hidden INTEGER",
    "ALTER TABLE `Contact` ADD COLUMN source TEXT",
    "ALTER TABLE `Contact` ADD COLUMN bookId VARCHAR(40)",
    "ALTER TABLE `Contact` ADD COLUMN etag TEXT",
];

// ============================================================================
// V7: Label and Folder createdAt/updatedAt columns
// Source: constants.h V7_SETUP_QUERIES
// ============================================================================

pub const V7_SETUP: &[&str] = &[
    "ALTER TABLE `Label` ADD COLUMN path TEXT",
    "ALTER TABLE `Label` ADD COLUMN role TEXT",
    "ALTER TABLE `Label` ADD COLUMN createdAt DATETIME",
    "ALTER TABLE `Label` ADD COLUMN updatedAt DATETIME",
    "ALTER TABLE `Folder` ADD COLUMN path TEXT",
    "ALTER TABLE `Folder` ADD COLUMN role TEXT",
    "ALTER TABLE `Folder` ADD COLUMN createdAt DATETIME",
    "ALTER TABLE `Folder` ADD COLUMN updatedAt DATETIME",
];

// ============================================================================
// V8: Thread additional columns for received/sent timestamps
// Source: constants.h V8_SETUP_QUERIES
// ============================================================================

pub const V8_SETUP: &[&str] = &[
    "ALTER TABLE `Thread` ADD COLUMN lastMessageReceivedTimestamp INTEGER",
    "ALTER TABLE `Thread` ADD COLUMN lastMessageSentTimestamp INTEGER",
    // ThreadListSortIndex uses lastMessageReceivedTimestamp (added in V8)
    "CREATE INDEX IF NOT EXISTS `ThreadListSortIndex` ON `Thread` (accountId, lastMessageReceivedTimestamp)",
];

// ============================================================================
// V9: ContactBook, ContactGroup, and ContactContactGroup tables
// Source: constants.h V9_SETUP_QUERIES
// ============================================================================

pub const V9_SETUP: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS `ContactGroup` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), bookId VARCHAR(40), data BLOB, version INTEGER, name TEXT)",
    "CREATE TABLE IF NOT EXISTS `ContactContactGroup` (id VARCHAR(40) PRIMARY KEY, value VARCHAR(40))",
    "CREATE TABLE IF NOT EXISTS `ContactBook` (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), data BLOB, version INTEGER)",
];

// ============================================================================
// Account reset queries
// Source: constants.h ACCOUNT_RESET_QUERIES
// These DELETE statements remove all data for a specific account (by accountId).
// They must be executed in dependency order — dependent tables first.
// After these queries, _State cleanup and VACUUM are performed separately.
// ============================================================================

pub const ACCOUNT_RESET_QUERIES: &[&str] = &[
    "DELETE FROM `ThreadCounts` WHERE `categoryId` IN (SELECT id FROM `Folder` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadCounts` WHERE `categoryId` IN (SELECT id FROM `Label` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadCategory` WHERE `id` IN (SELECT id FROM `Thread` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadSearch` WHERE `content_id` IN (SELECT id FROM `Thread` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadReference` WHERE `accountId` = ?",
    "DELETE FROM `Thread` WHERE `accountId` = ?",
    "DELETE FROM `File` WHERE `accountId` = ?",
    "DELETE FROM `Event` WHERE `accountId` = ?",
    "DELETE FROM `Label` WHERE `accountId` = ?",
    "DELETE FROM `MessageBody` WHERE `id` IN (SELECT id FROM `Message` WHERE `accountId` = ?)",
    "DELETE FROM `Message` WHERE `accountId` = ?",
    "DELETE FROM `Task` WHERE `accountId` = ?",
    "DELETE FROM `Folder` WHERE `accountId` = ?",
    "DELETE FROM `ContactSearch` WHERE `content_id` IN (SELECT id FROM `Contact` WHERE `accountId` = ?)",
    "DELETE FROM `Contact` WHERE `accountId` = ?",
    "DELETE FROM `Calendar` WHERE `accountId` = ?",
    "DELETE FROM `ModelPluginMetadata` WHERE `accountId` = ?",
    "DELETE FROM `DetatchedPluginMetadata` WHERE `accountId` = ?",
    "DELETE FROM `Account` WHERE `id` = ?",
];
