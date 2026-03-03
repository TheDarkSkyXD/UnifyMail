# Phase 8 Deep-Dive: C++ Source Analysis

## Overview

This document captures the exact implementation patterns from the C++ mailsync engine that the Rust rewrite (Phase 8) must replicate. All analysis is derived from direct reading of the source files in `app/mailsync/MailSync/`.

Source files analyzed:
- `SyncWorker.cpp` / `SyncWorker.hpp`
- `TaskProcessor.cpp` / `TaskProcessor.hpp`
- `Models/Task.cpp` / `Models/Task.hpp`
- `Models/MailModel.cpp`
- `DeltaStream.cpp` / `DeltaStream.hpp`
- `SyncException.cpp`
- `main.cpp`
- `app/frontend/flux/tasks/task.ts`
- `app/frontend/flux/mailsync-bridge.ts`
- `app/frontend/flux/stores/task-queue.ts`
- `app/frontend/flux/tasks/change-mail-task.ts`

---

## SyncWorker IDLE Implementation

### Threading Model

Three separate `SyncWorker` instances (background, foreground) plus DAV/contacts/metadata workers:

```
main thread:       runListenOnMainThread()  — stdin JSON parse loop
background thread: runBackgroundSyncWorker() → bgWorker->syncNow() in hard loop
foreground thread: runForegroundSyncWorker() → fgWorker->idleCycleIteration() in hard loop
calContacts thread: runCalContactsSyncWorker()
metadata thread:   metadataWorker->run()
metadataExpiration thread: metadataExpirationWorker->run()
```

The foreground worker (`fgWorker`) is only started AFTER the background worker completes its first full pass through `syncFoldersAndLabels()`. This guarantees the folder list and `uidnext`/`highestmodseq` values are populated before IDLE begins.

```cpp
// main.cpp lines 190–207
if (!started) {
    bgWorker->syncFoldersAndLabels();
    SharedDeltaStream()->endConnectionError(bgWorker->account->id());
    if (!fgThread) {
        fgThread = new std::thread([&]() {
            SetThreadName("foreground");
            fgWorker = make_shared<SyncWorker>(bgWorker->account);
            runForegroundSyncWorker();
        });
    }
    started = true;
}
```

### SyncWorker Constructor and Session Configuration

```cpp
// SyncWorker.cpp lines 54–71
SyncWorker::SyncWorker(shared_ptr<Account> account) :
    store(new MailStore()),
    account(account),
    unlinkPhase(1),
    logger(spdlog::get("logger")),
    processor(new MailProcessor(account, store)),
    session(IMAPSession())
{
    store->setStreamDelay(500);
}

void SyncWorker::configure()
{
    // For XOAuth2 accounts, may make HTTP requests — must be called inside retry handlers
    MailUtils::configureSessionForAccount(session, account);
}
```

Each `SyncWorker` has its own `IMAPSession`. The foreground and background workers are separate objects with separate sessions and stores. The store delay for the foreground worker is 500ms; for the main-thread store it is 5ms.

### IDLE State Machine: `idleCycleIteration()`

The IDLE cycle is a single function called in a loop from `runForegroundSyncWorker()`. Each call represents one complete IDLE iteration:

**Step 1: Drain body fetch queue (requested by UI via `need-bodies` command)**

```cpp
while (true) {
    string id;
    { lock(idleMtx); id = idleFetchBodyIDs.back(); idleFetchBodyIDs.pop_back(); }
    auto msg = store->find<Message>(byId);
    if (msg && session.isDisconnected()) {
        session.connectIfNeeded(&connectErr);
        session.loginIfNeeded(&connectErr);
    }
    syncMessageBody(msg.get());
}
```

**Step 2: Check `idleShouldReloop` early exits** — if the flag is set, clear it and return immediately without entering IDLE. The flag is set by `idleInterrupt()` which is called from the main thread.

**Step 3: Connect and login**

```cpp
session.connectIfNeeded(&err);  // throws SyncException if fails
session.loginIfNeeded(&err);    // throws SyncException if fails
```

**Step 4: Run remote tasks**

```cpp
TaskProcessor processor { account, store, &session };
processor.cleanupOldTasksAtRuntime();

int rowid = -1;
SQLite::Statement statement(store->db(),
    "SELECT rowid, data FROM Task WHERE accountId = ? AND status = \"remote\" AND rowid > ?");

do {
    tasks = {};
    statement.bind(1, account->id());
    statement.bind(2, rowid);
    while (statement.executeStep()) {
        tasks.push_back(make_shared<Task>(statement));
        rowid = max(rowid, statement.getColumn("rowid").getInt());
    }
    statement.reset();
    for (auto & task : tasks) {
        processor.performRemote(task.get());
    }
} while (tasks.size() > 0);
```

Key behaviors:
- Tracks the last `rowid` processed so newly created tasks (e.g., `SendDraftTask` spawning a `SyncbackMetadataTask`) are also processed in the same iteration.
- Runs in a `do...while` loop until no more "remote" tasks remain.
- `processor.cleanupOldTasksAtRuntime()` keeps only the last 100 completed/cancelled tasks.

**Step 5: Check `idleShouldReloop` again** — return early if set.

**Step 6: Find the IDLE folder**

```cpp
Query q = Query().equal("accountId", account->id()).equal("role", "inbox");
auto inbox = store->find<Folder>(q);
if (inbox == nullptr) {
    inbox = store->find<Folder>(Query().equal("accountId", account->id()).equal("role", "all"));
    if (inbox == nullptr) {
        throw SyncException("no-inbox", "...", false);
    }
}
```

Preference order: `inbox` role, then `all` role. Throws a non-retryable exception if neither exists.

**Step 7: Sync the IDLE folder (if initial sync has started)**

```cpp
bool hasStartedSyncingFolder = inbox->localStatus().count(LS_SYNCED_MIN_UID) > 0 &&
                               inbox->localStatus()[LS_SYNCED_MIN_UID].is_number();
if (hasStartedSyncingFolder) {
    // Process VANISHED notifications from the previous IDLE session
    IndexSet * idleVanished = session.idleVanishedMessages();
    if (idleVanished && idleVanished->count() > 0) {
        // unlinkMessagesMatchingQuery for UIDs in idleVanished
    }

    IMAPFolderStatus remoteStatus = session.folderStatus(&path, &err);

    if (session.storedCapabilities()->containsIndex(IMAPCapabilityCondstore)) {
        syncFolderChangesViaCondstore(*inbox, remoteStatus, false);
    } else {
        // UID-range based sync of top 100 messages
        uint32_t bottomUID = store->fetchMessageUIDAtDepth(*inbox, 100, uidnext);
        syncFolderUIDRange(*inbox, RangeMake(bottomUID, uidnext - bottomUID), false);
        inbox->localStatus()[LS_LAST_SHALLOW] = time(0);
        inbox->localStatus()[LS_UIDNEXT] = uidnext;
    }

    syncMessageBodies(*inbox, remoteStatus);
    store->saveFolderStatus(inbox.get(), inboxInitialStatus);
}
```

**Step 8: Enter IDLE or condition-variable wait**

```cpp
if (idleShouldReloop) { return; }

if (session.setupIdle()) {
    logger->info("Idling on folder {}", inbox->path());
    String path = AS_MCSTR(inbox->path());
    session.idle(&path, 0, &err);  // timeout = 0 (no timeout)
    session.unsetupIdle();
    logger->info("Idle exited with code {}", err);
    // Errors NOT thrown here — Yandex and others randomly close IDLE connections
} else {
    logger->info("Connection does not support idling. Locking until more to do...");
    std::unique_lock<std::mutex> lck(idleMtx);
    idleCv.wait(lck);  // wait indefinitely
}
```

**Critical note on IDLE timeout:** The `session.idle()` call is made with timeout = 0. This means no application-level timeout — the library drives the IDLE until the server responds or the connection drops. Yandex and other servers randomly close IDLE connections, and this is treated as normal; errors are logged but not thrown.

**IDLE without capability:** If the server does not support IDLE (detected via `session.setupIdle()` returning false), the foreground worker blocks on a condition variable until `idleInterrupt()` is called. This effectively freezes the foreground worker until woken by a task or wake-workers command.

### IDLE Interruption

Interruption is the mechanism used to wake the foreground worker when a task arrives or the user requests a sync:

```cpp
// SyncWorker.cpp lines 73–81
void SyncWorker::idleInterrupt()
{
    // called on main / random threads — do NOT call non-thread-safe functions from here!
    std::unique_lock<std::mutex> lck(idleMtx);
    idleShouldReloop = true;
    session.interruptIdle();  // MailCore2 call — sends DONE to the server
    idleCv.notify_one();      // wakes the cv.wait() in the non-IDLE path
}
```

There are three callers of `idleInterrupt()`:
1. `queue-task` handler (with 300ms delay to batch rapid task queuing)
2. `wake-workers` handler (immediate)
3. `need-bodies` handler (immediate)

The 300ms delay for task queuing is implemented with an `atomic<bool> queuedForegroundWake` flag and a detached thread:

```cpp
static atomic<bool> queuedForegroundWake { false };
bool expected = false;
if (queuedForegroundWake.compare_exchange_strong(expected, true)) {
    std::thread([]() {
        std::this_thread::sleep_for(chrono::milliseconds(300));
        if (fgWorker) { fgWorker->idleInterrupt(); }
        queuedForegroundWake = false;
    }).detach();
}
```

This ensures only one pending wake is scheduled even if many tasks arrive in quick succession.

### Body Fetch Queue

Body fetches are queued via:

```cpp
void SyncWorker::idleQueueBodiesToSync(vector<string> & ids) {
    std::unique_lock<std::mutex> lck(idleMtx);
    for (string & id : ids) {
        idleFetchBodyIDs.push_back(id);
    }
}
```

The main thread calls this and then immediately calls `idleInterrupt()`. The IDs are popped from the back of the vector (LIFO order) in `idleCycleIteration`.

### Foreground Worker Error Recovery

```cpp
// main.cpp lines 149–170
void runForegroundSyncWorker() {
    while(true) {
        try {
            fgWorker->configure();        // re-configures session on every iteration
            fgWorker->idleCycleIteration();
            SharedDeltaStream()->endConnectionError(fgWorker->account->id());
        } catch (SyncException & ex) {
            exceptions::logCurrentExceptionWithStackTrace();
            if (!ex.isRetryable()) {
                abort();  // Non-retryable errors kill the process
            }
            if (ex.isOffline()) {
                SharedDeltaStream()->beginConnectionError(fgWorker->account->id());
            }
            spdlog::get("logger")->info("--sleeping");
            MailUtils::sleepWorkerUntilWakeOrSec(120);  // 2 minute sleep with interrupt support
        } catch (...) {
            exceptions::logCurrentExceptionWithStackTrace();
            abort();
        }
    }
}
```

Key behaviors:
- `configure()` is called on every loop iteration, not just on first start. For OAuth2 accounts, this may make HTTP requests to refresh tokens.
- Non-retryable `SyncException` (e.g., authentication failures) call `abort()` to terminate the process.
- Retryable exceptions sleep for up to 120 seconds (interruptible via wake signal).
- Unknown exceptions (`...`) always call `abort()`.
- `endConnectionError()` emits a `ProcessState` delta to the UI on success.

### Background Worker Error Recovery

```cpp
void runBackgroundSyncWorker() {
    bool started = false;
    MailUtils::sleepWorkerUntilWakeOrSec(bgWorker->account->startDelay());  // staggered start

    while(true) {
        try {
            bgWorker->configure();
            if (!started || bgWorkerShouldMarkAll) {
                bgWorker->markAllFoldersBusy();
                bgWorkerShouldMarkAll = false;
            }
            if (!started) {
                bgWorker->syncFoldersAndLabels();
                // ... start fgThread after first successful folder sync
                started = true;
            }
            bool moreToSync = true;
            while(moreToSync) {
                moreToSync = bgWorker->syncNow();
            }
            SharedDeltaStream()->endConnectionError(bgWorker->account->id());
        } catch (SyncException & ex) {
            // same pattern as foreground
        } catch (...) {
            abort();
        }
        MailUtils::sleepWorkerUntilWakeOrSec(120);
    }
}
```

The background worker sleeps for `account->startDelay()` seconds before starting. Multiple accounts stagger their startup to avoid SQLite locking.

---

## TaskProcessor — Complete Task Type Map

### Local Phase: All task types handled by `performLocal()`

| Task Type | Local Phase Action |
|---|---|
| `ChangeUnreadTask` | `performLocalChangeOnMessages(_applyUnread)` — sets `msg->setUnread()`, increments `syncUnsavedChanges`, extends `syncedAt` by 24 hours |
| `ChangeStarredTask` | `performLocalChangeOnMessages(_applyStarred)` — sets `msg->setStarred()`, same lock pattern |
| `ChangeFolderTask` | `performLocalChangeOnMessages(_applyFolder)` — sets `msg->setClientFolder()`, same lock pattern |
| `ChangeLabelsTask` | `performLocalChangeOnMessages(_applyLabels)` — modifies `remoteXGMLabels` array, same lock pattern |
| `SyncbackDraftTask` | `performLocalSaveDraft()` — saves draft Message + body to `MessageBody` table |
| `DestroyDraftTask` | `performLocalDestroyDraft()` — removes drafts, creates deletion placeholder stubs in trash folder, stores stub IDs in task data |
| `SyncbackCategoryTask` | no-op |
| `DestroyCategoryTask` | no-op |
| `SendDraftTask` | no-op |
| `SyncbackMetadataTask` | `performLocalSyncbackMetadata()` — upserts metadata on model, stores `modelMetadataNewVersion` in task data |
| `SendFeatureUsageEventTask` | no-op |
| `ChangeRoleMappingTask` | `performLocalChangeRoleMapping()` — clears old role assignment, assigns new role to folder/label |
| `ExpungeAllInFolderTask` | no-op |
| `GetMessageRFC2822Task` | no-op |
| `EventRSVPTask` | no-op |
| `DestroyContactTask` | `performLocalDestroyContact()` — sets `contact->setHidden(true)` |
| `SyncbackContactTask` | `performLocalSyncbackContact()` — upserts Contact in local store, stores contact ID in task data |
| `ChangeContactGroupMembershipTask` | `performLocalChangeContactGroupMembership()` — modifies group membership in local store |
| `SyncbackContactGroupTask` | `performLocalSyncbackContactGroup()` — creates/updates ContactGroup (and underlying Contact for CardDAV) |
| `DestroyContactGroupTask` | `performLocalDestroyContactGroup()` — removes ContactGroup, stores `googleResourceName` in task data for Gmail |
| `SyncbackEventTask` | `performLocalSyncbackEvent()` — creates/updates Event from ICS, stores event ID in task data |
| `DestroyEventTask` | no-op (events not hidden locally, removal waits for remote completion) |

### Remote Phase: All task types handled by `performRemote()`

| Task Type | Remote Phase — IMAP/SMTP Commands | Notes |
|---|---|---|
| `ChangeUnreadTask` | `storeFlagsByUID(path, uids, IMAPStoreFlagsRequestKindAdd/Remove, MessageFlagSeen)` | Per-folder batch; remove Seen = mark unread, add Seen = mark read |
| `ChangeStarredTask` | `storeFlagsByUID(path, uids, IMAPStoreFlagsRequestKindAdd/Remove, MessageFlagFlagged)` | Per-folder batch |
| `ChangeFolderTask` | `moveMessages()` if `IMAPCapabilityMove` present, else `copyMessages()` + `storeFlagsByUID(Deleted)` + `expunge()` | Uses `_moveMessagesResilient()` helper; UIDPLUS extension used to track new UIDs |
| `ChangeLabelsTask` | `storeLabelsByUID(path, uids, Add, toAdd)` + `storeLabelsByUID(path, uids, Remove, toRemove)` | Gmail X-GM-LABELS extension; translates label roles to X-GM values |
| `SyncbackDraftTask` | no-op | Draft syncback not implemented |
| `DestroyDraftTask` | `_removeMessagesResilient(session, store, accountId, path, uids)` for each stub; then `store->remove(stub)` | Uses stubs created in local phase |
| `SyncbackCategoryTask` | `renameFolder()` (update) or `createFolder()` (new) | Handles namespace prefix and delimiter normalization |
| `DestroyCategoryTask` | `deleteFolder()` | |
| `SendDraftTask` | SMTP `sendMessage()` + IMAP `appendMessage()` + IMAP `fetchMessagesByUID()` | Full send flow — see detailed description below |
| `SyncbackMetadataTask` | HTTP POST to identity server `/metadata/{accountId}/{id}/{pluginId}` | Skipped if no identity |
| `SendFeatureUsageEventTask` | HTTP POST to identity server `/api/feature_usage_event` | |
| `ChangeRoleMappingTask` | no-op | Role is purely local |
| `ExpungeAllInFolderTask` | `storeFlagsByUID(path, 1:*, Add, Deleted)` + `expunge()` | Then deletes all local messages in batches of 100 with 300ms pauses |
| `GetMessageRFC2822Task` | `fetchMessageByUID()` + write to file at `data["filepath"]` | |
| `EventRSVPTask` | Builds RFC 6047 iMIP MIME message + SMTP `sendMessage()` | |
| `DestroyContactTask` | Google People API delete or CardDAV delete | |
| `SyncbackContactTask` | Google People API upsert or CardDAV `writeAndResyncContact()` | Skips contacts with `CONTACT_SOURCE_MAIL` |
| `ChangeContactGroupMembershipTask` | Google People API `updateContactGroupMembership()` or CardDAV `writeAndResyncContact()` | |
| `SyncbackContactGroupTask` | Google People API `upsertContactGroup()` or CardDAV `writeAndResyncContact()` | |
| `DestroyContactGroupTask` | Google People API group delete or CardDAV contact delete | |
| `SyncbackEventTask` | CalDAV `writeAndResyncEvent()` | |
| `DestroyEventTask` | CalDAV `deleteEvent()` for each event | |
| `SearchMessagesTask` | Opens new `IMAPSession`, `search(folderPath, IMAPSearchExpression::searchContent(query))` | Results stored in task data as `resultUIDs` and `resultCount` |
| `FetchQuotaTask` | Opens new `IMAPSession`, `getQuota(&usage, &limit)` | `ErrorCapability` = not supported, sets `data["supported"] = false` |

**Note:** `SearchMessagesTask` and `FetchQuotaTask` are only in `performRemote()` — they have no `performLocal()` branch at all (not listed in the `performLocal` dispatch).

### Detailed: SendDraftTask Remote Flow

```
1. Check _performRemoteRan flag — if already set, return immediately (idempotency guard)
2. Set _performRemoteRan = true, save task to DB
3. Load draft JSON from task data["draft"]
4. Load perRecipientBodies from task data["perRecipientBodies"]
5. inflateClientDraftJSON() — merge client JSON with local DB copy
6. Find "sent" Folder or Label (role = "sent")
7. Build MIME via MessageBuilder (to/cc/bcc/replyTo/from/subject/messageId/body/attachments)
8. If multisend: send separate SMTP messages per recipient (except "self")
   else: smtp.sendMessage(messageDataForSent)
9. If SMTP fails: throw SyncException("send-failed" or "send-partially-failed", retryable=false)
10. Delete remote draft via _removeMessagesResilient if remoteUID != 0
11. Search sent folder for message by headerMessageId (up to 4 tries, delays: 0,1,1,2 seconds)
12. If multisend and messages found: delete all copies via _removeMessagesResilient
    If single send and exactly 1 found: record sentFolderMessageUID
13. If no sentFolderMessageUID: appendMessage() to sent folder with MessageFlagSeen
14. For Gmail: apply thread labels to sent message via storeLabelsByUID
15. Re-select sent folder (Courier workaround for new message visibility)
16. fetchMessagesByUID() to get IMAP attributes (headers + flags + Gmail extensions)
17. processor.insertFallbackToUpdateMessage() to sync sent message into local store
18. processor.retrievedMessageBody() with the built MIME data
19. store->remove(&draft) to delete the local draft
20. For each metadata entry in draft: queue a SyncbackMetadataTask via performLocal()
```

### `performLocalChangeOnMessages()` — Sync Lock Pattern

All change-message tasks use this shared pattern:

```cpp
// For each message:
modifyLocalMessage(msg.get(), data);            // apply the change
msg->setSyncUnsavedChanges(msg->syncUnsavedChanges() + 1);  // increment lock
msg->setSyncedAt(time(0) + 24 * 60 * 60);      // set future syncedAt (24h ahead)
store->save(msg.get());
```

And on `performRemoteChangeOnMessages()`:

```cpp
// Reload messages inside a transaction after IMAP operations:
int suc = safe->syncUnsavedChanges() - 1;
safe->setSyncUnsavedChanges(suc);
if (suc == 0) {
    safe->setSyncedAt(time(0));  // reset syncedAt to now
}
store->save(safe.get());
// unsafeEraseTransactionDeltas() — suppresses delta emission for internal-only changes
```

The `syncedAt` being set 24 hours in the future prevents the background sync worker from overwriting locally-changed message state with stale server data.

When `updatesFolder = true` (for `ChangeFolderTask`), the reloaded message also gets `setRemoteUID()` and `setRemoteFolder()` from the in-memory (pre-reload) copy which was mutated by `_moveMessagesResilient`.

### `_moveMessagesResilient()` — Move Without MOVE Extension

```
If IMAPCapabilityMove present:
    session->moveMessages(srcPath, uids, destPath, &uidmap)
Else:
    session->copyMessages(srcPath, uids, destPath, &uidmap)
    session->storeFlagsByUID(srcPath, uids, Add, MessageFlagDeleted)
    session->expunge(srcPath)

If uidmap returned (UIDPLUS):
    update each msg->setRemoteUID(newUID) from uidmap

Else (no UIDPLUS):
    fetch last N messages from destPath by UID range
    match by computed Mailspring message ID
    update remoteUID for each matched message

If mustApplyAttributes (copy path only):
    re-apply Flagged/Seen/Draft flags to destination messages
```

### `_removeMessagesResilient()` — Permanent Delete

```
1. storeFlagsByUID(path, uids, Add, MessageFlagDeleted)
2. If IMAPCapabilityMove and trash folder exists:
   moveMessages(path, uids, trashPath, &uidMapping)
   If move succeeded and uidMapping returned:
     re-flag as Deleted in trash (Gmail removes Deleted on move)
     expungeUIDs(trashPath, uids)
   Else: expunge(path)
3. If no trash / move failed:
   expungeUIDs(path, uids) — falls back to expunge() if expungeUIDs fails
```

---

## Task JSON Wire Format

### `queue-task` stdin command

The main thread receives this from Electron:

```json
{
  "type": "queue-task",
  "task": {
    "__cls": "ChangeUnreadTask",
    "id": "temp-abc123",
    "aid": "account-id-here",
    "v": 0,
    "status": "local",
    "threadIds": ["thread-id-1"],
    "messageIds": [],
    "unread": false
  }
}
```

Processing in `runListenOnMainThread()`:

```cpp
if (type == "queue-task") {
    packet["task"]["v"] = 0;          // force version to 0 (new record)
    Task task{packet["task"]};
    processor.performLocal(&task);
    // then schedule idleInterrupt() after 300ms
}
```

### `cancel-task` stdin command

```json
{
  "type": "cancel-task",
  "taskId": "task-id-to-cancel"
}
```

Sets `data["should_cancel"] = true` on the task record. Checked in `performRemote()` before execution.

### `wake-workers` stdin command

```json
{
  "type": "wake-workers"
}
```

Sets `bgWorkerShouldMarkAll = true`, calls `MailUtils::wakeAllWorkers()`, and calls `fgWorker->idleInterrupt()`.

### `need-bodies` stdin command

```json
{
  "type": "need-bodies",
  "ids": ["message-id-1", "message-id-2"]
}
```

Pushes IDs into `fgWorker->idleFetchBodyIDs` and calls `fgWorker->idleInterrupt()`.

### `sync-calendar` stdin command

```json
{
  "type": "sync-calendar"
}
```

Spawns a detached thread running `DAVWorker::run()`.

### `detect-provider` stdin command

```json
{
  "type": "detect-provider",
  "email": "user@example.com",
  "requestId": "optional-request-id"
}
```

Response emitted to stdout:

```json
{
  "type": "provider-result",
  "requestId": "optional-request-id",
  "provider": {
    "identifier": "gmail",
    "servers": {
      "imap": [{ "hostname": "imap.gmail.com", "port": 993, "connectionType": 3 }],
      "smtp": [{ "hostname": "smtp.gmail.com", "port": 465, "connectionType": 3 }]
    }
  }
}
```

### `query-capabilities` stdin command

```json
{
  "type": "query-capabilities",
  "requestId": "optional-request-id"
}
```

Response:

```json
{
  "type": "capabilities-result",
  "requestId": "optional-request-id",
  "capabilities": {
    "idle": true,
    "condstore": true,
    "syncInProgress": true
  }
}
```

### `subscribe-folder-status` stdin command

```json
{
  "type": "subscribe-folder-status",
  "requestId": "optional-request-id",
  "folderIds": ["folder-id-1"]
}
```

Response:

```json
{
  "type": "folder-status",
  "requestId": "optional-request-id",
  "statuses": [
    {
      "folderId": "folder-id-1",
      "localStatus": { "uidnext": 12345, "syncedMinUID": 1, ... }
    }
  ]
}
```

### Task Data JSON — Field Reference

All Tasks inherit from `MailModel` with `_data` as the backing JSON store:

| Field | Type | Description |
|---|---|---|
| `__cls` | string | Constructor name / task type (e.g., `"ChangeUnreadTask"`) |
| `id` | string | Unique task ID (randomly generated in TypeScript) |
| `aid` | string | Account ID |
| `v` | int | Version (for SQLite INSERT vs UPDATE detection) |
| `status` | string | `"local"`, `"remote"`, `"complete"`, or `"cancelled"` |
| `error` | object | Set on failure: `{ what, key, debuginfo, retryable, offline }` |
| `should_cancel` | bool | Set by `cancel()` to signal the remote phase should skip |

Task-type-specific fields are stored directly in `_data` alongside the common fields.

**ChangeUnreadTask / ChangeStarredTask additional fields:**

```json
{
  "threadIds": ["thread-id-1", "thread-id-2"],
  "messageIds": [],
  "unread": false,
  "canBeUndone": true
}
```

**ChangeFolderTask:**

```json
{
  "threadIds": ["thread-id-1"],
  "messageIds": [],
  "folder": { "id": "folder-id", "path": "INBOX/Archive", "role": "archive", ... }
}
```

**ChangeLabelsTask (Gmail only):**

```json
{
  "threadIds": ["thread-id-1"],
  "labelsToAdd": [{ "id": "label-id", "path": "\\Important", "role": "important" }],
  "labelsToRemove": []
}
```

**SyncbackDraftTask:**

```json
{
  "draft": {
    "id": "draft-id",
    "aid": "account-id",
    "hMsgId": "<msg-id@domain.com>",
    "subject": "Hello",
    "to": [{ "name": "Alice", "email": "alice@example.com" }],
    "from": [{ "name": "Me", "email": "me@example.com" }],
    "cc": [], "bcc": [], "replyTo": [],
    "body": "<html>...</html>",
    "files": [],
    "plaintext": false
  }
}
```

**DestroyDraftTask:**

```json
{
  "messageIds": ["draft-id-1"],
  "stubIds": ["stub-id-1"]  // added by performLocal
}
```

**SendDraftTask:**

```json
{
  "draft": { ... },
  "perRecipientBodies": {
    "self": "<html>self body</html>",
    "alice@example.com": "<html>tracked body for alice</html>"
  }
}
```

**SyncbackMetadataTask:**

```json
{
  "modelId": "message-id",
  "modelClassName": "message",
  "modelHeaderMessageId": "<msg-id@domain.com>",
  "pluginId": "open-tracking",
  "value": { "uid": "tracking-uid", "expiration": 1234567890 },
  "modelMetadataNewVersion": 3  // added by performLocal
}
```

**SyncbackCategoryTask:**

```json
{
  "path": "Work/Projects",
  "existingPath": "Work/Old Projects"  // optional, if renaming
}
```

**DestroyCategoryTask:**

```json
{
  "path": "Work/Projects"
}
```

**ChangeRoleMappingTask:**

```json
{
  "path": "My Sent",
  "role": "sent"
}
```

**ExpungeAllInFolderTask:**

```json
{
  "folder": { "id": "folder-id", "path": "INBOX/Trash" }
}
```

**GetMessageRFC2822Task:**

```json
{
  "messageId": "message-id",
  "filepath": "/path/to/output.eml"
}
```

**EventRSVPTask:**

```json
{
  "ics": "BEGIN:VCALENDAR\r\nMETHOD:REPLY\r\n...",
  "subject": "Accepted: Meeting",
  "to": "organizer@example.com",
  "icsRSVPStatus": "ACCEPTED"
}
```

**SyncbackEventTask:**

```json
{
  "calendarId": "calendar-id",
  "event": {
    "id": "event-id",  // optional, omit for create
    "ics": "BEGIN:VCALENDAR\r\n..."
  }
}
```

**DestroyEventTask:**

```json
{
  "events": [{ "id": "event-id-1" }, { "id": "event-id-2" }]
}
```

**SearchMessagesTask:**

```json
{
  "query": "search terms",
  "folderId": "optional-folder-id",
  "resultUIDs": [1234, 1235],  // added by performRemote
  "resultCount": 2              // added by performRemote
}
```

**FetchQuotaTask:**

```json
{
  "supported": true,    // added by performRemote
  "usageKB": 1024,      // added by performRemote
  "limitKB": 15360      // added by performRemote
}
```

---

## Task Lifecycle State Machine

### Status Strings

```
"local"     — task has been received via stdin but performLocal has NOT run yet
              (Note: despite the name, "local" status means the task is PENDING)
"remote"    — performLocal succeeded; task is waiting for performRemote
"complete"  — performRemote succeeded (or task was cancelled during local/remote)
"cancelled" — performRemote set this status because shouldCancel() returned true
```

Important: The TypeScript `Task.Status.Local = 'local'` is confusing naming. The comment in TypeScript says "means the task has NOT run the local phase yet". After `performLocal` succeeds, status becomes `"remote"`. After `performRemote` succeeds, status becomes `"complete"`.

### State Transitions

```
Created (v=0, status="local")
    ↓ main thread: performLocal()
    ↓   → store->save(task) first (persists with status="local")
    ↓   → run local phase
    ↓   → on success: task->setStatus("remote")
    ↓   → on SyncException: task->setStatus("complete"), task->setError(ex.toJSON())
    ↓   → store->save(task)

status="remote"
    ↓ foreground thread: performRemote()
    ↓   → if shouldCancel(): task->setStatus("cancelled")
    ↓   → else: run remote phase
    ↓   → on success: task->setStatus("complete")
    ↓   → on SyncException: task->setStatus("complete"), task->setError(ex.toJSON())
    ↓   → store->save(task)

status="complete" or "cancelled"
    → emitted to Electron via DeltaStream
    → Electron calls task.onError() or task.onSuccess()
    → kept in DB until cleanupOldTasksAtRuntime() removes > 100 completed tasks
```

### Crash Recovery on Startup

```cpp
// TaskProcessor::cleanupTasksAfterLaunch()
auto stuck = store->findAll<Task>(
    Query().equal("accountId", account->id()).equal("status", "local")
);
for (auto & t : stuck) {
    store->remove(t.get());  // DELETE tasks stuck in "local" status
}
cleanupOldTasksAtRuntime();
```

Tasks with status `"local"` at startup are considered crashed mid-performLocal and are **deleted** (not retried). Tasks with status `"remote"` are re-processed in the next `idleCycleIteration()`.

### Task Persistence in SQLite

The Task table has these columns:

```sql
CREATE TABLE Task (
    id TEXT PRIMARY KEY,
    data TEXT,       -- full JSON blob including all task fields
    accountId TEXT,
    version INTEGER,
    status TEXT      -- indexed for fast "remote" task queries
)
```

The `status` column is stored both inside `data` JSON and as a separate indexed column for efficient SQL queries.

---

## DeltaStream: Output Format and Buffering

### Output Format

Each line on stdout is a complete JSON object followed by `\n`:

```json
{"type":"persist","modelJSONs":[{...}],"modelClass":"Message"}
{"type":"unpersist","modelJSONs":[{"id":"thread-id"}],"modelClass":"Thread"}
```

Special non-model deltas:

```json
{"type":"persist","modelJSONs":[{"accountId":"aid","id":"aid","connectionError":true}],"modelClass":"ProcessState"}
{"type":"persist","modelJSONs":[{...accountJSON...}],"modelClass":"ProcessAccountSecretsUpdated"}
```

### Buffering and Coalescing

The `DeltaStream` singleton manages buffering:

- `emit(item, maxDeliveryDelay)` adds to an internal buffer and schedules a flush within `maxDeliveryDelay` milliseconds.
- `flushWithin(ms)` schedules a background flush thread; if a flush is already scheduled for a *later* time, it notifies the condition variable to flush earlier.
- Multiple saves of the same object within a flush window are merged via `upsertModelJSON()` — the later save overwrites fields of the earlier save, but fields present only in the earlier save (e.g., `message.body`) are preserved.
- Deltas are grouped by `modelClass` in the buffer map. Within each class, consecutive items of the same `type` are concatenated into a single `DeltaStreamItem`.

The foreground `SyncWorker` is constructed with `store->setStreamDelay(500)` — a 500ms flush delay. The main-thread store uses 5ms.

### DeltaStream Singleton

```cpp
shared_ptr<DeltaStream> _globalStream = make_shared<DeltaStream>();
shared_ptr<DeltaStream> SharedDeltaStream() { return _globalStream; }
```

All workers share one `DeltaStream` instance. Thread safety is managed via `bufferMtx` (protects the buffer map) and `bufferFlushMtx` (protects the flush thread condition variable).

### stdin Parsing: `waitForJSON()`

```cpp
json DeltaStream::waitForJSON() {
    string buffer;
    cin.clear();
    cin.sync();
    getline(cin, buffer);
    if (buffer.size() > 0) {
        json j = json::parse(buffer);
        return j;
    }
    return {};
}
```

Each stdin message is a single line of JSON. The main thread calls this in a loop. If `cin` becomes bad (parent process died), the loop checks `lostCINAt` and exits after 30 seconds.

---

## Error Handling Patterns

### SyncException

All IMAP/SMTP/network errors are wrapped in `SyncException`:

```cpp
// SyncException.cpp
SyncException::SyncException(string key, string di, bool retryable)

// Constructed from MailCore2 ErrorCode:
SyncException::SyncException(mailcore::ErrorCode c, string di)
// ErrorConnection → retryable=true, offline=true
// ErrorParse → retryable=true (abrupt connection termination)
// ErrorFetch → retryable=true (abrupt connection termination)

// Constructed from CURLcode:
SyncException::SyncException(CURLcode c, string di)
// Network codes → retryable=true, offline=true

// JSON form:
{
  "what": "std::exception::what()",
  "key": "ErrorCode string",
  "debuginfo": "context string",
  "retryable": true,
  "offline": false
}
```

### Task Error Handling

Both `performLocal()` and `performRemote()` catch `SyncException` but NOT generic exceptions:

```cpp
} catch (SyncException & ex) {
    task->setError(ex.toJSON());
    task->setStatus("complete");  // tasks always complete, even on error
}
// any other exception propagates up to the worker retry loop
```

Task failures always result in status `"complete"` with an error object. There is no "failed" or "error" status — the Electron side infers failure from the presence of `task.error`.

### Non-Retryable Errors

Non-retryable `SyncException` in the worker retry loops calls `abort()`:

```cpp
if (!ex.isRetryable()) {
    abort();  // terminates the entire mailsync process
}
```

This signals Electron's crash tracker to record the crash and potentially stop restarting the account.

### Specific Error Keys Used in Tasks

| Key | Meaning |
|---|---|
| `"no-inbox"` | No inbox or all-mail folder for IDLE (foreground) |
| `"no-drafts-folder"` | Draft save with no drafts folder assigned |
| `"no-trash-folder"` | Draft destroy with no trash folder assigned |
| `"no-sent-folder"` | Send draft with no sent folder assigned |
| `"no-self-body"` | Multisend missing "self" body |
| `"send-failed"` | SMTP error, entire send failed |
| `"send-partially-failed"` | SMTP error after some recipients received it |
| `"not-found"` | Model not found in DB for syncback |
| `"no-matching-folder"` | ChangeRoleMapping folder not found |
| `"generic"` | General task validation errors |
| `"invalid-ics"` | ICS data fails RFC validation |
| `"missing-json"` | EventRSVPTask missing required fields |

---

## Crash Recovery

### Process Crash Recovery Sequence

1. Electron detects mailsync exit (code or signal other than SIGTERM).
2. `CrashTracker` records timestamp. If 5+ crashes in 5 minutes: account marked as `SYNC_STATE_ERROR` or `SYNC_STATE_AUTH_FAILED`.
3. Otherwise: `ensureClients()` is called, which relaunches the mailsync process.
4. On relaunch: `cleanupTasksAfterLaunch()` runs before any sync work.
5. Tasks in status `"local"` are deleted (lost forever — no retry).
6. Tasks in status `"remote"` are processed normally in the next `idleCycleIteration()`.
7. The background worker marks all folders busy and re-syncs from scratch.

### SQLite Consistency

The C++ engine is the exclusive writer to SQLite. The Electron renderer is read-only. There is no database locking conflict between crash and recovery. SQLite WAL mode allows concurrent reads during writes.

### Orphan Detection

If `stdin` (the connection to Electron) disconnects for more than 30 seconds and the process was launched without `--orphan`, the process calls `std::exit(141)`:

```cpp
if (time(0) - lostCINAt > 30) {
    std::exit(141);
}
```

This distinguishes normal process termination from being orphaned after a parent crash.

---

## Key Differences to Account For in Rust

### 1. IDLE Timeout Handling

The C++ code passes timeout = 0 to `session.idle()`, relying on the MailCore2 library to handle IDLE timeout (RFC 2177 requires servers to send DONE after 29 minutes). The Rust implementation using `async-imap` or similar will need to explicitly implement a 25–28 minute timeout and re-enter IDLE.

### 2. Thread Model Mapping

The C++ uses raw `std::thread` + `std::mutex` + `std::condition_variable`. The Rust implementation should use `tokio` tasks. The `idleMtx`/`idleCv`/`idleShouldReloop` pattern maps to a `tokio::sync::Notify` or `tokio::sync::watch` channel.

### 3. Task Rowid Tracking

The C++ uses SQLite `rowid` to track "tasks I've seen in this pass". The Rust implementation must replicate this: tasks spawned by other tasks (e.g., `SendDraftTask` spawning `SyncbackMetadataTask`) must also be processed in the same pass. The `rowid > ?` query with the tracked max rowid is the mechanism.

### 4. `syncUnsavedChanges` / `syncedAt` Locking

The 24-hour `syncedAt` extension prevents the background sync from overwriting optimistic local changes. The Rust implementation must implement this same mechanism when processing change tasks to avoid flicker.

### 5. Delta Coalescing

The `upsertModelJSON()` function merges JSON objects by key, preserving keys present only in earlier saves (like `message.body`). This is not a simple replace — the Rust DeltaStream implementation must replicate this merge logic.

### 6. Task Cleanup on Launch

Rust implementation must replicate `cleanupTasksAfterLaunch()`: delete all tasks with status `"local"`, keep only the last 100 completed/cancelled. This runs on the main thread before sync workers start.

### 7. `_performRemoteRan` Idempotency Guard for SendDraftTask

The C++ sets `_performRemoteRan = true` at the start of `performRemoteSendDraft()` and immediately saves the task. If the process crashes and the task is re-queued, it returns early. The Rust implementation must replicate this field and check.

### 8. Detached vs. Re-used IMAP Sessions

`SearchMessagesTask` and `FetchQuotaTask` open a *new* `IMAPSession` rather than reusing the foreground worker's session. This avoids disrupting the IDLE state. The Rust implementation should open a fresh connection for these tasks.

### 9. MIME Message Assembly for SendDraft

The C++ uses MailCore2's `MessageBuilder` for most headers, then calls `builder.data()` to get a serialized MIME blob. For `EventRSVPTask`, it manually constructs `multipart/alternative` MIME with a `text/plain` and `text/calendar; method=REPLY` part, builds headers via `MessageBuilder`, extracts headers, replaces Content-Type manually, and concatenates with the multipart body. This manual MIME assembly requires careful handling in Rust.

### 10. No Retry for Task Remote Phase

There is no retry loop for individual task execution. If `performRemote` throws a `SyncException`, the task is immediately marked complete with an error. Retry is only at the worker loop level (which restarts after 120 seconds for retryable connection errors). The Rust implementation must not add per-task retry logic.

### 11. CalContacts Thread Independence

The CalContacts thread does NOT use `MailUtils::sleepWorkerUntilWakeOrSec()` — it sleeps for 45 minutes between runs using plain `std::this_thread::sleep_for`. It is not woken by `wake-workers` commands. The Google API daily limit triggers a 4-hour sleep; auth failures (401/403) cause the thread to exit entirely.

### 12. Foreground Worker Starts After First Background Pass

The foreground IDLE worker is only started after `syncFoldersAndLabels()` completes on the background thread. The Rust implementation must enforce this ordering to avoid IDLE on a folder list that doesn't exist yet.

### 13. `store->unsafeEraseTransactionDeltas()`

After `performRemoteChangeOnMessages()` saves the updated remote attributes (remoteUID, remoteFolder, syncUnsavedChanges), it calls `unsafeEraseTransactionDeltas()` to suppress emitting those changes to the UI. The UI does not need to react to internal sync state fields. The Rust DeltaStream must support this selective suppression.
