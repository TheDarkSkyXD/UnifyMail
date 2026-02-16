const fs = require('fs');
const path = require('path');

const args = process.argv;
const modeIndex = args.indexOf('--mode');
const mode = modeIndex !== -1 ? args[modeIndex + 1] : 'unknown';

// Extract config-dir-path from environment (set by mailsync-process.ts)
const configDirPath = process.env.CONFIG_DIR_PATH || null;

const V1_SETUP = [
    "CREATE TABLE IF NOT EXISTS `_State` (id VARCHAR(40) PRIMARY KEY, value TEXT)",
    "CREATE TABLE IF NOT EXISTS `File` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), filename TEXT)",
    "CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), calendarId VARCHAR(40), _start INTEGER, _end INTEGER, is_search_indexed INTEGER DEFAULT 0)",
    "CREATE INDEX IF NOT EXISTS EventIsSearchIndexedIndex ON `Event` (is_search_indexed, id)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `EventSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, title, description, location, participants)",
    `CREATE TABLE IF NOT EXISTS Label (
      id VARCHAR(40) PRIMARY KEY,
      accountId VARCHAR(8),
      version INTEGER,
      data TEXT,
      path VARCHAR(255),
      role VARCHAR(255),
      createdAt DATETIME,
      updatedAt DATETIME)`,
    `CREATE TABLE IF NOT EXISTS Folder (
       id VARCHAR(40) PRIMARY KEY,
       accountId VARCHAR(8),
       version INTEGER,
       data TEXT,
       path VARCHAR(255),
       role VARCHAR(255),
       createdAt DATETIME,
       updatedAt DATETIME)`,
    `CREATE TABLE IF NOT EXISTS Thread (
        id VARCHAR(42) PRIMARY KEY,
        accountId VARCHAR(8),
        version INTEGER,
        data TEXT,
        gThrId VARCHAR(20),
        subject VARCHAR(500),
        snippet VARCHAR(255),
        unread INTEGER,
        starred INTEGER,
        firstMessageTimestamp DATETIME,
        lastMessageTimestamp DATETIME,
        lastMessageReceivedTimestamp DATETIME,
        lastMessageSentTimestamp DATETIME,
        inAllMail TINYINT(1),
        isSearchIndexed TINYINT(1),
        participants TEXT,
        hasAttachments INTEGER)`,
    "CREATE INDEX IF NOT EXISTS ThreadDateIndex ON `Thread` (lastMessageReceivedTimestamp DESC)",
    "CREATE INDEX IF NOT EXISTS ThreadUnreadIndex ON `Thread` (accountId, lastMessageReceivedTimestamp DESC) WHERE unread = 1 AND inAllMail = 1",
    "CREATE INDEX IF NOT EXISTS ThreadUnifiedUnreadIndex ON `Thread` (lastMessageReceivedTimestamp DESC) WHERE unread = 1 AND inAllMail = 1",
    "CREATE INDEX IF NOT EXISTS ThreadStarredIndex ON `Thread` (accountId, lastMessageReceivedTimestamp DESC) WHERE starred = 1 AND inAllMail = 1",
    "CREATE INDEX IF NOT EXISTS ThreadUnifiedStarredIndex ON `Thread` (lastMessageReceivedTimestamp DESC) WHERE starred = 1 AND inAllMail = 1",
    "CREATE INDEX IF NOT EXISTS ThreadGmailLookup ON `Thread` (gThrId) WHERE gThrId IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS ThreadIsSearchIndexedIndex ON `Thread` (isSearchIndexed, id)",
    "CREATE INDEX IF NOT EXISTS ThreadIsSearchIndexedLastMessageReceivedIndex ON `Thread` (isSearchIndexed, lastMessageReceivedTimestamp)",
    `CREATE TABLE IF NOT EXISTS ThreadReference (
        threadId VARCHAR(42),
        accountId VARCHAR(8),
        headerMessageId VARCHAR(255),
        PRIMARY KEY (threadId, accountId, headerMessageId))`,
    `CREATE TABLE IF NOT EXISTS ThreadCategory (
        id VARCHAR(40),
        value VARCHAR(40),
        inAllMail TINYINT(1),
        unread TINYINT(1),
        lastMessageReceivedTimestamp DATETIME,
        lastMessageSentTimestamp DATETIME,
        PRIMARY KEY (id, value))`,
    "CREATE INDEX IF NOT EXISTS `ThreadCategory_id` ON `ThreadCategory` (`id` ASC)",
    "CREATE UNIQUE INDEX IF NOT EXISTS `ThreadCategory_val_id` ON `ThreadCategory` (`value` ASC, `id` ASC)",
    "CREATE INDEX IF NOT EXISTS ThreadListCategoryIndex ON `ThreadCategory` (lastMessageReceivedTimestamp DESC, value, inAllMail, unread, id)",
    "CREATE INDEX IF NOT EXISTS ThreadListCategorySentIndex ON `ThreadCategory` (lastMessageSentTimestamp DESC, value, inAllMail, unread, id)",
    "CREATE TABLE IF NOT EXISTS `ThreadCounts` (`categoryId` TEXT PRIMARY KEY, `unread` INTEGER, `total` INTEGER)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ThreadSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, subject, to_, from_, categories, body)",
    "CREATE TABLE IF NOT EXISTS `Account` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email_address TEXT)",
    `CREATE TABLE IF NOT EXISTS Message (
        id VARCHAR(40) PRIMARY KEY,
        accountId VARCHAR(8),
        version INTEGER,
        data TEXT,
        headerMessageId VARCHAR(255),
        gMsgId VARCHAR(255),
        gThrId VARCHAR(255),
        subject VARCHAR(500),
        date DATETIME,
        draft TINYINT(1),
        unread TINYINT(1),
        starred TINYINT(1),
        remoteUID INTEGER,
        remoteXGMLabels TEXT,
        remoteFolderId VARCHAR(40),
        replyToHeaderMessageId VARCHAR(255),
        threadId VARCHAR(40))`,
    "CREATE INDEX IF NOT EXISTS MessageListThreadIndex ON Message(threadId, date ASC)",
    "CREATE INDEX IF NOT EXISTS MessageListHeaderMsgIdIndex ON Message(headerMessageId)",
    "CREATE INDEX IF NOT EXISTS MessageListDraftIndex ON Message(accountId, date DESC) WHERE draft = 1",
    "CREATE INDEX IF NOT EXISTS MessageListUnifiedDraftIndex ON Message(date DESC) WHERE draft = 1",
    "CREATE TABLE IF NOT EXISTS `ModelPluginMetadata` (id VARCHAR(40), `accountId` VARCHAR(8), `objectType` VARCHAR(15), `value` TEXT, `expiration` DATETIME, PRIMARY KEY (`value`, `id`))",
    "CREATE INDEX IF NOT EXISTS `ModelPluginMetadata_id` ON `ModelPluginMetadata` (`id` ASC)",
    "CREATE INDEX IF NOT EXISTS `ModelPluginMetadata_expiration` ON `ModelPluginMetadata` (`expiration` ASC) WHERE expiration IS NOT NULL",
    "CREATE TABLE IF NOT EXISTS `DetatchedPluginMetadata` (objectId VARCHAR(40), objectType VARCHAR(15), accountId VARCHAR(8), pluginId VARCHAR(40), value BLOB, version INTEGER, PRIMARY KEY (`objectId`, `accountId`, `pluginId`))",
    "CREATE TABLE IF NOT EXISTS `MessageBody` (id VARCHAR(40) PRIMARY KEY, `value` TEXT)",
    "CREATE UNIQUE INDEX IF NOT EXISTS MessageBodyIndex ON MessageBody(id)",
    "CREATE TABLE IF NOT EXISTS `Contact` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email TEXT, version INTEGER, refs INTEGER DEFAULT 0)",
    "CREATE INDEX IF NOT EXISTS ContactEmailIndex ON Contact(email)",
    "CREATE INDEX IF NOT EXISTS ContactAccountEmailIndex ON Contact(accountId, email)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ContactSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, content)",
    "CREATE TABLE IF NOT EXISTS `Calendar` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8))",
    "CREATE TABLE IF NOT EXISTS `Task` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), status VARCHAR(255))"
];

const V2_SETUP = [
    "CREATE INDEX IF NOT EXISTS MessageUIDScanIndex ON Message(accountId, remoteFolderId, remoteUID)"
];

const V3_SETUP = [
    "ALTER TABLE `MessageBody` ADD COLUMN fetchedAt DATETIME",
    "UPDATE `MessageBody` SET fetchedAt = datetime('now')"
];

const V4_SETUP = [
    "DELETE FROM Task WHERE Task.status = \"complete\" OR Task.status = \"cancelled\"",
    "CREATE INDEX IF NOT EXISTS TaskByStatus ON Task(accountId, status)"
];

const V6_SETUP = [
    "DROP TABLE IF EXISTS `Event`",
    "CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), etag VARCHAR(40), calendarId VARCHAR(40), recurrenceStart INTEGER, recurrenceEnd INTEGER)",
    "CREATE INDEX IF NOT EXISTS EventETag ON Event(calendarId, etag)"
];

const V7_SETUP = [
    "ALTER TABLE `Event` ADD COLUMN icsuid VARCHAR(150)",
    "CREATE INDEX IF NOT EXISTS EventUID ON Event(accountId, icsuid)"
];

const V8_SETUP = [
    "DELETE FROM Contact WHERE refs = 0;",
    "ALTER TABLE `Contact` ADD COLUMN hidden TINYINT(1) DEFAULT 0",
    "ALTER TABLE `Contact` ADD COLUMN source VARCHAR(10) DEFAULT 'mail'",
    "ALTER TABLE `Contact` ADD COLUMN bookId VARCHAR(40)",
    "ALTER TABLE `Contact` ADD COLUMN etag VARCHAR(40)",
    "CREATE INDEX IF NOT EXISTS ContactBrowseIndex ON Contact(hidden,refs,accountId)",
    "CREATE TABLE `ContactGroup` (`id` varchar(40),`accountId` varchar(40),`bookId` varchar(40), `data` BLOB, `version` INTEGER, `name` varchar(300), PRIMARY KEY (id))",
    "CREATE TABLE `ContactContactGroup` (`id` varchar(40),`value` varchar(40), PRIMARY KEY (id, value));",
    "CREATE TABLE `ContactBook` (`id` varchar(40),`accountId` varchar(40), `data` BLOB, `version` INTEGER, PRIMARY KEY (id));"
];

const V9_SETUP = [
    "ALTER TABLE `Event` ADD COLUMN recurrenceId VARCHAR(50) DEFAULT ''",
    "CREATE INDEX IF NOT EXISTS EventRecurrenceId ON Event(calendarId, icsuid, recurrenceId)"
];

const CURRENT_VERSION = 9;

if (mode === 'migrate') {
    if (configDirPath) {
        const dbPath = path.join(configDirPath, 'edgehill.db');
        try {
            // Ensure directory exists
            if (!fs.existsSync(configDirPath)) {
                fs.mkdirSync(configDirPath, { recursive: true });
            }

            const Database = require(path.join(__dirname, '..', 'app', 'node_modules', 'better-sqlite3'));
            const db = new Database(dbPath);
            db.pragma('journal_mode = WAL');

            const version = db.pragma('user_version', { simple: true });
            console.log(JSON.stringify({ log: `Current DB version: ${version}` }));

            const runQueries = (queries) => {
                for (const sql of queries) {
                    try {
                        db.exec(sql);
                    } catch (e) {
                        console.log(JSON.stringify({ log: `Migration error on: ${sql} - ${e.message}` }));
                    }
                }
            };

            if (version < 1) runQueries(V1_SETUP);
            if (version < 2) runQueries(V2_SETUP);
            if (version < 3) runQueries(V3_SETUP);
            if (version < 4) runQueries(V4_SETUP);
            if (version < 6) runQueries(V6_SETUP);
            if (version < 7) runQueries(V7_SETUP);
            if (version < 8) runQueries(V8_SETUP);
            if (version < 9) runQueries(V9_SETUP);

            if (version < CURRENT_VERSION) {
                db.pragma(`user_version = ${CURRENT_VERSION}`);
            }

            db.close();
            console.log(JSON.stringify({ log: `Migration completed to version ${CURRENT_VERSION}` }));

        } catch (err) {
            console.log(JSON.stringify({ log: `Fatal Migration Error: ${err.message}` }));
            process.exit(1);
        }
    }
    console.log(JSON.stringify({ log: "Migration successful (MOCKED)" }));
    process.exit(0);
} else {
    // Sync mode mock
    console.log(JSON.stringify({ log: `Mock Mailsync Started in ${mode} mode` }));
    setInterval(() => { }, 10000); // Keep alive
    process.stdin.resume();
    process.on('SIGINT', () => process.exit(0));
}
