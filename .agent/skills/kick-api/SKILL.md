---
name: kick-api
description: Integration guide for the Kick.com Public API (v1). Covers Authentication (OAuth 2.1), Channels, Livestreams, Chat, and Categories. Use when the user asks about Kick API, endpoints, or building Kick apps.
---

# Kick API Skill

## When to use this skill
- User asks about **Kick.com API** or endpoints.
- User needs to **authenticate** with Kick (OAuth 2.1).
- User wants to **fetch channel, user, or stream data** from Kick.
- User wants to **integrate chat** (send messages, listen to events).

## API Fundamentals

- **Base URL**: `https://api.kick.com/public/v1`
- **Documentation**: [https://docs.kick.com](https://docs.kick.com) | [GitHub Ops](https://github.com/KickEngineering/KickDevDocs)
- **Authentication**: OAuth 2.1 (Authorization Code flow with PKCE).
- **Rate Limits**: Refer to headers `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`.

## Authentication (OAuth 2.1)

Kick uses OAuth 2.1. You must register an application in the Kick Developer Portal to get a Client ID and Client Secret.

### 1. Authorization Request
Redirect user to:
```
https://id.kick.com/oauth/authorize
  ?client_id={CLIENT_ID}
  &redirect_uri={REDIRECT_URI}
  &response_type=code
  &scope={SCOPES}
  &code_challenge={PKCE_CHALLENGE}
  &code_challenge_method=S256
```

**Common Scopes:**
- `user:read`
- `channel:read`
- `chat:write`
- `stream:read`

### 2. Exchange Code for Token
POST to `https://id.kick.com/oauth/token`
```json
{
  "grant_type": "authorization_code",
  "client_id": "...",
  "client_secret": "...",
  "code": "...",
  "redirect_uri": "...",
  "code_verifier": "..."
}
```

### 3. Usage
Include the token in the Authorization header:
`Authorization: Bearer {ACCESS_TOKEN}`

## Key Endpoints

### Channels
- **Get Channel**: `GET /channels/{channel_slug}`
- **Search/List**: `GET /channels?broadcaster_user_id={id}`

### Users
- **Get User**: `GET /users/{user_id}` or `GET /users/{username}` (Verify exact param in docs)
- **Get Me**: `GET /users/me` (requires user scope)

### Livestreams
- **Get Live Streams**: `GET /livestreams` (params: `broadcaster_user_id`, `category_id`)
- **Check Stream State**: Check `is_live` field in Channel object or use Livestreams endpoint.

### Chat
- **Send Message**: `POST /chat`
  ```json
  {
    "content": "Hello world",
    "type": "user" // or "bot"
  }
  ```
- **Delete Message**: `DELETE /chat/{message_id}`

### Categories
- **List Categories**: `GET /categories`
- **Get Category**: `GET /categories/{category_id}`

## Events (Webhooks/WebSocket)

Kick supports webhooks for real-time events.
- **Webhook Events**:
  - `chat.message.sent`
  - `livestream.started`
  - `livestream.ended`
  - `channel.followed`
  - `subscription.new`
  
(Note: Always verify latest events in official docs as the API is evolving).

## Example Pattern (TypeScript)

```typescript
async function getKickChannel(slug: string, token: string) {
  const response = await fetch(`https://api.kick.com/public/v1/channels/${slug}`, {
    headers: {
      'Authorization': `Bearer ${token}`,
      'Accept': 'application/json'
    }
  });

  if (!response.ok) {
    throw new Error(`Kick API Error: ${response.statusText}`);
  }

  return response.json();
}
```

## Resources
- **Official Docs**: https://docs.kick.com/
- **Developer Portal**: https://dev.kick.com (check availability)
