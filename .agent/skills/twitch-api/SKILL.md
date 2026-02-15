---
name: twitch-api
description: Interacting with the Twitch API (Helix). Use when the user wants to fetch streams, users, channels, or manage chat integration.
---

# Twitch API (Helix) Skill

## When to use this skill
- Fetching live stream data (titles, view counts, thumbnails).
- Getting user information (profile pics, channel IDs).
- Checking stream status (online/offline).
- Managing chat/moderation via API (not IRC).
- Implementing EventSub webhooks (high level).

## Authentication
All Helix API requests require two headers:
1. **Client-ID**: Your registered application's client ID.
2. **Authorization**: `Bearer <access_token>`.

### OAuth Token Types
- **App Access Token** (Client Credentials): Best for server-side fetching of public data (streams, users).
  - POST `https://id.twitch.tv/oauth2/token`
  - Body: `client_id=<id>&client_secret=<secret>&grant_type=client_credentials`
- **User Access Token** (Authorization Code): Required for acting on behalf of a user (chat, follow, sub).

## Base URL
`https://api.twitch.tv/helix`

## Common Endpoints

### 1. Users
**Endpoint:** `GET /users`
- **Params:** `id` (User ID) or `login` (Username). Max 100.
- **Why use it:** To resolve usernames to IDs (stable identifier) and get profile pictures (`profile_image_url`).
- **Example:** `GET /users?login=ninja`

### 2. Streams (Live Status)
**Endpoint:** `GET /streams`
- **Params:** `user_id` or `user_login` or `game_id`.
- **Why use it:** To check if a user is live.
- **Note:** If the `data` array is empty, the user is **offline**.
- **Thumbnails:** URLs look like `...live_user_ninja-{width}x{height}.jpg`. Replace `{width}x{height}` (e.g., `1280x720`) to display.

### 3. Channel Information
**Endpoint:** `GET /channels`
- **Params:** `broadcaster_id` (Required).
- **Why use it:** To get stream title and game name even when the channel is **offline**.

### 4. Search Categories (Games)
**Endpoint:** `GET /search/categories`
- **Params:** `query` (search term).
- **Why use it:** To find `game_id` for category filtering.

## Implementation Guidelines

### Fetch Wrapper Example (TypeScript)
```typescript
async function fetchTwitch(endpoint: string, params: Record<string, string> = {}) {
  const url = new URL(`https://api.twitch.tv/helix/${endpoint}`);
  Object.entries(params).forEach(([k, v]) => url.searchParams.append(k, v));

  const res = await fetch(url.toString(), {
    headers: {
      'Client-ID': process.env.TWITCH_CLIENT_ID!,
      'Authorization': `Bearer ${process.env.TWITCH_ACCESS_TOKEN!}`
    }
  });

  if (!res.ok) throw new Error(`Twitch API Error: ${res.status} ${await res.text()}`);
  return res.json();
}
```

## Best Practices
1. **IDs over Names**: Store `user_id`. Usernames (`login`) can change.
2. **Pagination**: If a response contains `pagination: { cursor: "..." }`, pass `after=<cursor>` in the next request to load more.
3. **Rate Limits**: Standard bucket is 800 points/minute. Most requests allow 100 items per call. Always fetch in batches when possible.
4. **Image Caching**: React components should handle image loading specifically for thumbnails, as they change often.

## Resources
- [Official Documentation](https://dev.twitch.tv/docs/api/)
- [API Reference](https://dev.twitch.tv/docs/api/reference)
