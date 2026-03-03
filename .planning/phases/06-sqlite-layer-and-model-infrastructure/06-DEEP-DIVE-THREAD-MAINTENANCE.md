# Phase 6 Deep Dive: Thread Maintenance Algorithm

**Researched:** 2026-03-03
**Confidence:** HIGH — all algorithms traced line-by-line from C++ source
**Scope:** `Thread::applyMessageAttributeChanges()`, `MessageSnapshot`, `captureInitialState()`, `ThreadCounts` diff, `categoriesSearchString()`, participant merge, label resolution

---

## Overview

Thread state maintenance is the most complex lifecycle hook in the MailStore. When a message is saved or removed, `Message::afterSave()` / `Message::afterRemove()` call `Thread::applyMessageAttributeChanges()` to update the parent thread's ref-counted fields (unread, starred, attachmentCount, folders, labels, timestamps, participants).

The algorithm uses a **snapshot-diff-patch** approach:
1. A `MessageSnapshot` captures the message's state at load time
2. On save, the old snapshot is decremented from the thread, the new state is incremented
3. On removal, the old snapshot is decremented with no increment (next=nullptr)

---

## MessageSnapshot Struct

Source: `Message.hpp` lines 35-42

```cpp
struct MessageSnapshot {
    bool unread;
    bool starred;
    bool inAllMail;
    size_t fileCount;
    json remoteXGMLabels;   // nlohmann::json array of strings (Gmail label names)
    string clientFolderId;  // string ID of the folder object
};
```

**Static default:** `MessageEmptySnapshot = {false, false, false, 0, nullptr, ""}` — `remoteXGMLabels = nullptr` (JSON null, not empty array). Iterating over JSON null is a no-op, so the empty snapshot contributes nothing.

### Rust Equivalent

```rust
pub struct MessageSnapshot {
    pub unread: bool,
    pub starred: bool,
    pub in_all_mail: bool,
    pub file_count: usize,
    pub remote_xgm_labels: serde_json::Value, // json array or null
    pub client_folder_id: String,
}

pub const MESSAGE_EMPTY_SNAPSHOT: MessageSnapshot = MessageSnapshot {
    unread: false, starred: false, in_all_mail: false,
    file_count: 0,
    remote_xgm_labels: serde_json::Value::Null,
    client_folder_id: String::new(),
};
```

---

## Where _lastSnapshot Is Initialized

Source: `Message.cpp` constructors

| Constructor | Context | `_lastSnapshot` value |
|---|---|---|
| From IMAP (line 57) | Brand-new message from mail server | `MessageEmptySnapshot` |
| From SQLite (line 129) | Loading existing message from DB | `getSnapshot()` — captures current data |
| From JSON (line 136) | Task-constructed or IPC message | `MessageEmptySnapshot` if version==0, else `getSnapshot()` |

---

## Message::getSnapshot()

Source: `Message.cpp` lines 147-156

```cpp
MessageSnapshot Message::getSnapshot() {
    return MessageSnapshot {
        .unread = isUnread(),           // _data["unread"]
        .starred = isStarred(),         // _data["starred"]
        .inAllMail = inAllMail(),       // computed: clientFolder()["role"] != "spam" && != "trash"
        .fileCount = fileCountForThreadList(),  // see below
        .remoteXGMLabels = remoteXGMLabels(),   // _data["labels"] (copy)
        .clientFolderId = clientFolderId(),      // _data["folder"]["id"]
    };
}
```

**`fileCountForThreadList()`** — counts `_data["files"]` entries where `contentId` is null OR `size > 12 * 1024`. This excludes small inline images (CID attachments under 12KB) from the thread-level attachment count.

**`inAllMail()`** — computed from `clientFolder()["role"]`: returns `true` unless role is `"spam"` or `"trash"`. This is NOT a stored field — it's derived from the folder role.

---

## Message::afterSave() — The Trigger

Source: `Message.cpp` lines 449-469

```
1. MailModel::afterSave(store)                    // metadata join table
2. if _skipThreadUpdatesAfterSave → return        // batch optimization flag
3. if threadId() == "" → return                   // orphan message
4. thread = store->find<Thread>(threadId)
5. if thread == nullptr → return                  // thread deleted
6. allLabels = store->allLabelsCache(accountId)
7. thread->applyMessageAttributeChanges(_lastSnapshot, this, allLabels)
8. store->save(thread)                            // triggers Thread::afterSave()
9. _lastSnapshot = getSnapshot()                  // update snapshot for next diff
```

**`_skipThreadUpdatesAfterSave`** — public bool field on Message, defaults to `false`. Set to `true` in `TaskProcessor::performLocalChangeOnMessages()` when bulk-modifying messages, to defer thread updates to a full rebalance pass afterward.

---

## Message::afterRemove() — Removal Trigger

Source: `Message.cpp` lines 471-497

```
1. MailModel::afterRemove(store)                  // metadata join table cleanup
2. if threadId() == "" → return
3. thread = store->find<Thread>(threadId)
4. if thread == nullptr → return
5. allLabels = store->allLabelsCache(accountId)
6. thread->applyMessageAttributeChanges(_lastSnapshot, nullptr, allLabels)
   // nullptr = "message is gone" — decrement only, no increment
7. if thread->folders().size() == 0:
      store->remove(thread)                       // no messages left → delete thread
   else:
      store->save(thread)
8. DELETE FROM MessageBody WHERE id = ?           // cleanup body
```

**Critical:** `_skipThreadUpdatesAfterSave` is NOT checked in `afterRemove`. Removal always propagates.

---

## Thread::applyMessageAttributeChanges() — Complete Algorithm

Source: `Thread.cpp` lines 167-324

**Signature:**
```cpp
void applyMessageAttributeChanges(
    MessageSnapshot & old,                      // message state BEFORE change
    Message * next,                             // message AFTER change (nullptr = removal)
    vector<shared_ptr<Label>> allLabels         // all Label objects for account
);
```

### Phase 1: Decrement Scalar Counters

```cpp
setUnread(unread() - old.unread);           // bool→int: subtract 0 or 1
setStarred(starred() - old.starred);
setAttachmentCount(attachmentCount() - (int)old.fileCount);
```

No bounds checking — counters can go negative if data is inconsistent.

### Phase 2: Decrement Folder Refcount

Thread maintains `_data["folders"]` as a JSON array where each folder has augmented fields:
- `_refs` — how many messages in this thread are in this folder
- `_u` — how many of those messages are unread

```
for each folder f in folders():
    if f["id"] != old.clientFolderId:
        keep f unchanged
    else:
        refs = f["_refs"]
        if refs > 1:
            f["_refs"] = refs - 1
            f["_u"] = f["_u"] - old.unread
            keep f
        else (refs <= 1):
            DROP folder entirely (last message left this folder)
```

**Edge case:** If `old.clientFolderId` is empty (from `MessageEmptySnapshot`), no folder matches → all folders pass through unchanged.

### Phase 3: Decrement Label Refcounts

Labels come from Gmail's X-GM-LABELS. Each label name in `old.remoteXGMLabels` must be resolved to a Label object via `MailUtils::labelForXGMLabelName()`.

```
for each label_name in old.remoteXGMLabels:
    label = labelForXGMLabelName(label_name, allLabels)
    if label == nullptr → skip

    for each l in labels():
        if l["id"] != label.id():
            keep l
        else:
            refs = l["_refs"]
            if refs > 1:
                l["_refs"] = refs - 1
                l["_u"] = l["_u"] - old.unread  (see BUG NOTE below)
                keep l
            else (refs <= 1):
                DROP label
```

**BUG NOTE — Operator Precedence:**
The C++ code at line 215:
```cpp
l["_u"] = l["_u"].get<int>() - old.unread && old.inAllMail;
```
Parses as: `l["_u"] = (l["_u"].get<int>() - old.unread) && old.inAllMail;`
This produces 0 or 1 (boolean), NOT the correct arithmetic result. This is a latent bug in the C++ code. The same bug exists on the increment side (line 289). **The Rust implementation should replicate this behavior for C++ database compatibility, or document the intentional fix.**

**Edge case:** If `old.remoteXGMLabels` is JSON null (from `MessageEmptySnapshot`), the range-for loop iterates zero times → no labels decremented.

### Phase 4: Increment New State (only if `next != nullptr`)

All increments are guarded by `if (next)`.

#### 4a. Scalar Increments

```cpp
setUnread(unread() + next->isUnread());
setStarred(starred() + next->isStarred());
setAttachmentCount(attachmentCount() + (int)next->fileCountForThreadList());
```

#### 4b. Timestamp Updates

**Guard:** `if (!next->isDraft() && !next->isDeletionPlaceholder())`

```
lmt (lastMessageTimestamp):
    if next->date() > lastMessageTimestamp() → _data["lmt"] = next->date()

fmt (firstMessageTimestamp):
    if next->date() < firstMessageTimestamp() → _data["fmt"] = next->date()

lmst (lastMessageSentTimestamp):
    Guard: next->isSentByUser() && !next->isHiddenReminder()
    if next->date() > lastMessageSentTimestamp() → _data["lmst"] = next->date()

lmrt (lastMessageReceivedTimestamp) — COMPLEX:
    if next->isInInbox() || !next->isSentByUser():
        // "Real" received message
        if _data has "lmrt_is_fallback" OR next->date() > lmrt():
            erase "lmrt_is_fallback"
            _data["lmrt"] = next->date()
    elif lmrt() == 0:
        // No real received msg yet; use sent msg date as fallback
        _data["lmrt_is_fallback"] = true
        _data["lmrt"] = next->date()
```

**`lmrt_is_fallback`** — transient JSON key (not a column). Signals that lmrt is from a sent message and should be replaced by any real received message's date, regardless of date ordering.

**Supporting accessors:**
- `isSentByUser()` — `remoteFolder["role"] == "sent"`, or folder role is "all" with X-GM-LABELS containing "sent" (case-insensitive substring)
- `isHiddenReminder()` — from-name ends with "via Mailspring" (snooze/reminder feature)
- `isInInbox()` — `clientFolder()["role"] == "inbox"`
- `isDeletionPlaceholder()` — message is a tombstone marker

#### 4c. Folder Refcount Increment

```
clientFolderId = next->clientFolderId()
found = false
for each f in folders():
    if f["id"] == clientFolderId:
        f["_refs"] += 1
        f["_u"] += next->isUnread()
        found = true
if !found:
    f = next->clientFolder()   // full folder JSON from _data["folder"]
    f["_refs"] = 1
    f["_u"] = next->isUnread() ? 1 : 0
    folders().push_back(f)
```

**Note:** The folder increment loop does NOT break after finding — continues iterating. In practice only one entry matches.

#### 4d. Label Refcount Increment

```
for each label_name in next->remoteXGMLabels():
    label = labelForXGMLabelName(label_name, allLabels)
    if label == nullptr → skip

    found = false
    for each l in labels():
        if l["id"] == label.id():
            l["_refs"] += 1
            l["_u"] = l["_u"] + next->isUnread() && next->inAllMail()  // BUG: same precedence issue
            found = true
            break  // NOTE: DOES break here, unlike folder increment
    if !found:
        l = label->toJSON()    // full Label JSON blob
        l["_refs"] = 1
        l["_u"] = (next->isUnread() && next->inAllMail()) ? 1 : 0  // correct in new-label branch
        labels().push_back(l)
```

**Key difference from folders:** Label unread (`_u`) requires `inAllMail` guard — messages in spam/trash don't contribute to label unread counts. Folder unread does NOT have this guard.

#### 4e. Participant Merge

```
emails = set of all existing participant emails
addMissingParticipants(emails, next->to())
addMissingParticipants(emails, next->cc())
addMissingParticipants(emails, next->from())
// NOTE: bcc and replyTo are deliberately excluded
```

`addMissingParticipants()` iterates the contact JSON array, appends any contact whose email is not already in the set.

### Phase 5: Recompute `inAllMail` (ALWAYS runs, even if next=nullptr)

```
spamOrTrash = count of folders where role == "spam" or "trash"
_data["inAllMail"] = folders().size() > spamOrTrash
```

True unless EVERY folder is spam or trash. If `folders().size() == 0`, result is `false`.

---

## Full Thread Rebalance Pattern

Source: `TaskProcessor.cpp` lines 712-727

When a task modifies many messages in bulk, per-message `afterSave()` thread updates are suppressed via `_skipThreadUpdatesAfterSave = true`. After all messages are saved:

```
for each thread:
    thread->resetCountedAttributes()    // zeros unread, starred, attachmentCount, folders, labels
for each message:
    thread->applyMessageAttributeChanges(MessageEmptySnapshot, msg, allLabels)
    // EmptySnapshot → no decrement, full increment → accumulates from scratch
for each thread:
    store->save(thread)
```

`resetCountedAttributes()` (Thread.cpp lines 157-165): Zeros all ref-counted fields. Does NOT reset timestamps (lmt, fmt, lmst, lmrt).

---

## Thread::captureInitialState()

Source: `Thread.cpp` lines 448-452

```cpp
void Thread::captureInitialState() {
    _initialLMST = lastMessageSentTimestamp();
    _initialLMRT = lastMessageReceivedTimestamp();
    _initialCategoryIds = captureCategoryIDs();
}
```

Called from BOTH Thread constructors (new thread and DB-loaded thread). Captures the pre-mutation baseline for `afterSave()` to diff against.

---

## Thread::captureCategoryIDs()

Source: `Thread.cpp` lines 437-446

```cpp
map<string, bool> captureCategoryIDs() {
    map<string, bool> result{};
    for (auto & f : folders()):
        result[f["id"]] = f["_u"] > 0    // true if any unread in this folder
    for (auto & l : labels()):
        result[l["id"]] = l["_u"] > 0    // true if any unread in this label
    return result;
}
```

Returns `map<categoryId, hasUnread>` combining both folders and labels.

---

## Thread::afterSave() — ThreadCategory and ThreadCounts Maintenance

Source: `Thread.cpp` lines 348-416

```
1. MailModel::afterSave(store)                    // metadata join table
2. categoryIds = captureCategoryIDs()             // current state
3. if categoryIds changed OR lmrt changed OR lmst changed:
     DELETE FROM ThreadCategory WHERE id = threadId
     for each (catId, unreadBool) in categoryIds:
         INSERT INTO ThreadCategory (id, value, inAllMail, unread, lmrt, lmst)
         VALUES (threadId, catId, inAllMail, unreadBool, lmrt, lmst)
4. if categoryIds changed (membership, not just timestamps):
     // COMPUTE DIFF:
     diffs = {}
     Phase A: for each (catId, wasUnread) in _initialCategoryIds:
         diffs[catId] = [-wasUnread, -1]   // negative: thread was here, remove contribution
     Phase B: for each (catId, isUnread) in categoryIds:
         if catId in diffs:                 // exists in both old and new
             diffs[catId] = [diffs[catId][0] + isUnread, 0]  // total delta = -1+1 = 0
         else:                              // new category
             diffs[catId] = [isUnread, +1]
     Phase C: for each (catId, [unread_delta, total_delta]) in diffs:
         if both == 0 → skip
         UPDATE ThreadCounts SET unread = unread + unread_delta, total = total + total_delta
         WHERE categoryId = catId

     // UPDATE FTS5:
     if searchRowId() != 0:
         UPDATE ThreadSearch SET categories = categoriesSearchString()
         WHERE rowid = searchRowId
```

### ThreadCounts Diff Truth Table

| Old State | New State | unread_delta | total_delta |
|---|---|---|---|
| absent | present, read | 0 | +1 |
| absent | present, unread | +1 | +1 |
| present, read | absent | 0 | -1 |
| present, unread | absent | -1 | -1 |
| present, read | present, read | 0 | 0 (skipped) |
| present, read | present, unread | +1 | 0 |
| present, unread | present, read | -1 | 0 |
| present, unread | present, unread | 0 | 0 (skipped) |

---

## Thread::categoriesSearchString()

Source: `Thread.cpp` lines 136-155

```
result = ""
for each folder f in folders():
    if f["role"] is non-empty → result += role + " "
    else → result += f["path"] + " "
for each label l in labels():
    if l["role"] is non-empty → result += role + " "
    else → result += l["path"] + " "
return result
```

Produces a space-separated string of folder/label identifiers. Role names (inbox, sent, trash, spam, all) are preferred over IMAP paths.

---

## MailUtils::labelForXGMLabelName() — Label Resolution

Source: `MailUtils.cpp` lines 449-479

```
1. First pass: exact path match
   for each label in allLabels:
       if label.path() == mlname → return label

2. Second pass: backslash-prefixed system labels (\Inbox, \Sent, etc.)
   if mlname starts with "\\":
       strip backslash, lowercase
       for each label:
           lowercase(label.path())
           strip "[Gmail]/" prefix if present
           if path == mlname → return label
           if label.role() == mlname OR label.role() == mlname + "s" → return label

3. Not found → log warning, return nullptr
```

The "role + s" match handles: `\Sent` → role `"sent"`, `\Draft` → role `"drafts"`.

---

## Thread::afterRemove()

Source: `Thread.cpp` lines 418-432

```
1. MailModel::afterRemove(store)    // metadata cleanup
2. afterSave(store)                 // clears ThreadCategory + decrements ThreadCounts
   // Works because the thread's folders/labels are now empty (all messages removed)
   // so captureCategoryIDs() returns {} which diffs against _initialCategoryIds
3. if searchRowId() > 0:
     DELETE FROM ThreadSearch WHERE rowid = searchRowId
```

---

## MailStore::allLabelsCache()

Source: `MailStore.cpp` lines 299-306

```cpp
vector<shared_ptr<Label>> allLabelsCache(string accountId) {
    if (_labelCacheVersion != globalLabelsVersion) {
        _labelCache = findAll<Label>(Query().equal("accountId", accountId));
        _labelCacheVersion = globalLabelsVersion;
    }
    return _labelCache;
}
```

- `globalLabelsVersion` — `atomic<int>` starting at 1, incremented on every Label save/remove
- `_labelCacheVersion` — starts at 0, updated when cache is refreshed
- Single-account assumption: cache stores labels for only one accountId at a time
- **Rust:** Use `AtomicI32` or simpler `i32` (since single-writer thread). Per MailStore instance, one account.

---

## Rust Implementation Requirements Summary

| Component | Phase 6 Scope | Notes |
|---|---|---|
| `MessageSnapshot` struct | YES | 6 fields, constructed at Message load time |
| `Message::getSnapshot()` | YES | Reads from _data fields |
| `Message::_lastSnapshot` | YES | Transient field, not persisted |
| `Thread::applyMessageAttributeChanges()` | YES | Core algorithm: 5 phases |
| `Thread::captureInitialState()` | YES | Called in constructor |
| `Thread::captureCategoryIDs()` | YES | Returns map from folders + labels |
| `Thread::afterSave()` ThreadCategory + ThreadCounts | YES | Diff algorithm |
| `Thread::categoriesSearchString()` | YES | FTS5 search string builder |
| `Thread::afterRemove()` | YES | Delegates to afterSave + ThreadSearch cleanup |
| `Message::_skipThreadUpdatesAfterSave` | YES | Public bool flag |
| `Thread::resetCountedAttributes()` | YES | For bulk rebalance (TaskProcessor) |
| `MailUtils::labelForXGMLabelName()` | YES | Label resolution from X-GM-LABELS |
| `MailStore::allLabelsCache()` | YES | Cached label lookup with atomic invalidation |
| `isSentByUser()`, `isInInbox()`, etc. | YES | Message accessors for timestamp logic |
