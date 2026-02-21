/**
 * Cloudflare Worker for UnifyMail Authentication
 * 
 * Handles secure token exchange for Gmail and other providers.
 * Secrets are stored here and never exposed to the client.
 */

// Define environment interface
interface Env {
    GMAIL_CLIENT_ID: string;
    GMAIL_CLIENT_SECRET: string;
    // Add other provider secrets here as needed
}

export default {
    async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
        const url = new URL(request.url);
        const path = url.pathname;

        // CORS Headers
        const corsHeaders = {
            'Access-Control-Allow-Origin': '*', // Lock this down to specific domains if needed, or 'null' for local files
            'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
            'Access-Control-Allow-Headers': 'Content-Type',
        };

        if (request.method === 'OPTIONS') {
            return new Response(null, { headers: corsHeaders });
        }

        // Route: /auth/gmail/token
        // Description: Exchanges authorization code for access/refresh tokens
        if (path === '/auth/gmail/token' && request.method === 'POST') {
            try {
                const body = await request.json() as any;
                const code = body.code;
                const redirect_uri = body.redirect_uri || 'http://127.0.0.1:12141';

                if (!code) {
                    return new Response(JSON.stringify({ error: 'Missing code' }), {
                        status: 400,
                        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                    });
                }

                // Prepare request to Google
                const tokenUrl = 'https://oauth2.googleapis.com/token';
                const params = new URLSearchParams();
                params.append('code', code);
                params.append('client_id', env.GMAIL_CLIENT_ID);
                params.append('client_secret', env.GMAIL_CLIENT_SECRET);
                params.append('redirect_uri', redirect_uri);
                params.append('grant_type', 'authorization_code');

                // Execute token exchange
                console.log(`Exchanging code for tokens (Client ID: ${env.GMAIL_CLIENT_ID})`);
                const tokenResponse = await fetch(tokenUrl, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                    body: params
                });

                const tokenData = await tokenResponse.json();

                if (!tokenResponse.ok) {
                    console.error('Token exchange failed:', JSON.stringify(tokenData));
                    return new Response(JSON.stringify({
                        error: 'Token exchange failed',
                        details: tokenData
                    }), {
                        status: tokenResponse.status,
                        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                    });
                }

                // Return tokens to client
                return new Response(JSON.stringify(tokenData), {
                    status: 200,
                    headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                });

            } catch (err: any) {
                console.error('Error in /auth/gmail/token:', err);
                return new Response(JSON.stringify({ error: err.message }), {
                    status: 500,
                    headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                });
            }
        }

        // 404 for unknown routes
        return new Response('Not Found', { status: 404, headers: corsHeaders });
    }
};
