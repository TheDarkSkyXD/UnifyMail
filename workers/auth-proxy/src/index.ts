/**
 * Cloudflare Worker for UnifyMail Authentication
 *
 * Handles:
 * - Secure OAuth token exchange (client secret stays server-side)
 * - Token refresh for Gmail
 * - Compliance pages required by Google OAuth verification (homepage, privacy policy, ToS)
 */

// Define environment interface
interface Env {
    GMAIL_CLIENT_ID: string;
    GMAIL_CLIENT_SECRET: string;
    // Add other provider secrets here as needed
}

// ─── HTML Page Templates ─────────────────────────────────────────────────────

function htmlPage(title: string, body: string): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${title} - UnifyMail</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; background: #f8f9fa; }
        .container { max-width: 800px; margin: 0 auto; padding: 40px 20px; }
        .header { text-align: center; margin-bottom: 40px; }
        .header h1 { font-size: 2rem; color: #1a73e8; }
        .header p { color: #666; margin-top: 8px; }
        .content { background: white; border-radius: 12px; padding: 40px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
        .content h2 { color: #1a73e8; margin: 24px 0 12px; }
        .content h2:first-child { margin-top: 0; }
        .content p, .content li { margin-bottom: 8px; }
        .content ul { padding-left: 24px; }
        .content a { color: #1a73e8; }
        .footer { text-align: center; margin-top: 40px; color: #999; font-size: 0.875rem; }
        .badge { display: inline-block; background: #e8f0fe; color: #1a73e8; padding: 4px 12px; border-radius: 20px; font-size: 0.875rem; margin-top: 8px; }
        .cta { display: inline-block; background: #1a73e8; color: white; padding: 12px 24px; border-radius: 8px; text-decoration: none; margin-top: 16px; }
        .cta:hover { background: #1557b0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>UnifyMail</h1>
            <p>Open-source email client for desktop and mobile</p>
        </div>
        <div class="content">${body}</div>
        <div class="footer">
            <p>&copy; ${new Date().getFullYear()} UnifyMail. Open source under MIT License.</p>
            <p><a href="/">Home</a> | <a href="/privacy">Privacy Policy</a> | <a href="/terms">Terms of Service</a></p>
        </div>
    </div>
</body>
</html>`;
}

const HOMEPAGE_BODY = `
<h2>Your email, unified</h2>
<p>UnifyMail is a free, open-source email client that brings all your email accounts together in one place. Available for Windows, macOS, and Linux.</p>
<span class="badge">Open Source</span>

<h2>Features</h2>
<ul>
    <li><strong>Multi-account support</strong> &mdash; Gmail, Outlook, Office 365, Yahoo, IMAP/SMTP, and more</li>
    <li><strong>Unified inbox</strong> &mdash; See all your email in one view</li>
    <li><strong>Fast search</strong> &mdash; Find any email instantly</li>
    <li><strong>Themes &amp; plugins</strong> &mdash; Customize your experience</li>
    <li><strong>Calendar invites</strong> &mdash; View and RSVP to calendar invites in emails</li>
    <li><strong>Contact management</strong> &mdash; Built-in address book</li>
    <li><strong>Privacy-first</strong> &mdash; Your data stays on your machine</li>
</ul>

<h2>How it works with Google</h2>
<p>When you connect a Gmail account, UnifyMail uses Google's OAuth 2.0 to securely authenticate. We request access to:</p>
<ul>
    <li><strong>Email access</strong> &mdash; To sync, read, and send your email via IMAP/SMTP</li>
    <li><strong>Profile info</strong> &mdash; Your name and email address for account setup</li>
    <li><strong>Contacts</strong> &mdash; To show contact details alongside emails</li>
</ul>
<p>Your credentials and tokens are stored locally on your device. UnifyMail does not store your data on any server. Token exchange is handled through a secure proxy that only holds the OAuth client secret.</p>

<h2>Get started</h2>
<p>Download UnifyMail from our GitHub repository:</p>
<a class="cta" href="https://github.com/nicorithink/UnifyMail" target="_blank">View on GitHub</a>
`;

const PRIVACY_BODY = `
<h2>Privacy Policy</h2>
<p><strong>Last updated:</strong> ${new Date().toISOString().split('T')[0]}</p>

<h2>Overview</h2>
<p>UnifyMail is a desktop email client application. We are committed to protecting your privacy. This policy explains what data we access, how we use it, and your rights.</p>

<h2>Data We Access</h2>
<p>When you connect an email account (such as Gmail), UnifyMail accesses:</p>
<ul>
    <li><strong>Email messages</strong> &mdash; To display, search, and manage your email locally</li>
    <li><strong>Profile information</strong> &mdash; Your name and email address, used for account identification</li>
    <li><strong>Contacts</strong> &mdash; To display sender/recipient information alongside emails</li>
</ul>

<h2>How We Use Your Data</h2>
<ul>
    <li>All email data is synced and stored <strong>locally on your device</strong> only</li>
    <li>We do <strong>not</strong> upload, transmit, or store your email content on any external server</li>
    <li>OAuth tokens are stored locally on your device in an encrypted configuration</li>
    <li>The only server-side component is a token exchange proxy that forwards your authorization code to Google's servers and returns the tokens to your device. This proxy does not log or store any tokens.</li>
</ul>

<h2>Data Storage</h2>
<ul>
    <li>All data is stored locally in your application data directory</li>
    <li>No cloud storage, no telemetry, no analytics</li>
    <li>Uninstalling the application removes all local data</li>
</ul>

<h2>Data Sharing</h2>
<p>We do <strong>not</strong> share, sell, or transfer your data to any third parties. Period.</p>

<h2>Third-Party Services</h2>
<p>UnifyMail connects to the following services solely for email functionality:</p>
<ul>
    <li><strong>Google APIs</strong> &mdash; For Gmail OAuth authentication and IMAP/SMTP access</li>
    <li><strong>Microsoft APIs</strong> &mdash; For Outlook/Office 365 OAuth authentication and IMAP/SMTP access</li>
    <li><strong>Your email provider's IMAP/SMTP servers</strong> &mdash; For email sync and sending</li>
</ul>

<h2>Data Retention &amp; Deletion</h2>
<p>Since all data is stored locally:</p>
<ul>
    <li>Remove an account from UnifyMail to delete its local data</li>
    <li>Uninstall the application to remove all data</li>
    <li>Revoke UnifyMail's access in your Google Account settings at <a href="https://myaccount.google.com/permissions" target="_blank">myaccount.google.com/permissions</a></li>
</ul>

<h2>Children's Privacy</h2>
<p>UnifyMail is not directed at children under 13. We do not knowingly collect data from children.</p>

<h2>Changes to This Policy</h2>
<p>We may update this policy from time to time. Changes will be reflected on this page with an updated date.</p>

<h2>Contact</h2>
<p>For privacy questions or concerns, contact us at: <a href="mailto:theaiistheworld@gmail.com">theaiistheworld@gmail.com</a></p>

<h2>Google API Services Disclosure</h2>
<p>UnifyMail's use and transfer of information received from Google APIs adheres to the <a href="https://developers.google.com/terms/api-services-user-data-policy" target="_blank">Google API Services User Data Policy</a>, including the Limited Use requirements.</p>
`;

const TERMS_BODY = `
<h2>Terms of Service</h2>
<p><strong>Last updated:</strong> ${new Date().toISOString().split('T')[0]}</p>

<h2>Acceptance of Terms</h2>
<p>By using UnifyMail, you agree to these terms. If you do not agree, do not use the application.</p>

<h2>Description of Service</h2>
<p>UnifyMail is a free, open-source desktop email client that connects to your email accounts via standard protocols (IMAP, SMTP) and OAuth 2.0 authentication.</p>

<h2>Your Accounts</h2>
<ul>
    <li>You are responsible for maintaining the security of your email accounts</li>
    <li>UnifyMail stores your authentication credentials locally on your device</li>
    <li>You can revoke access at any time through your email provider's settings</li>
</ul>

<h2>Acceptable Use</h2>
<p>You agree not to use UnifyMail to:</p>
<ul>
    <li>Send spam or unsolicited bulk email</li>
    <li>Violate any applicable laws or regulations</li>
    <li>Infringe on the rights of others</li>
</ul>

<h2>Open Source License</h2>
<p>UnifyMail is distributed under the MIT License. The source code is available on GitHub. You may modify and redistribute the software in accordance with the license terms.</p>

<h2>Disclaimer of Warranties</h2>
<p>UnifyMail is provided "as is" without warranty of any kind. We do not guarantee uninterrupted or error-free operation.</p>

<h2>Limitation of Liability</h2>
<p>To the maximum extent permitted by law, UnifyMail and its contributors shall not be liable for any indirect, incidental, or consequential damages arising from your use of the application.</p>

<h2>Changes to Terms</h2>
<p>We may update these terms from time to time. Continued use of the application constitutes acceptance of updated terms.</p>

<h2>Contact</h2>
<p>For questions about these terms, contact: <a href="mailto:theaiistheworld@gmail.com">theaiistheworld@gmail.com</a></p>
`;

// ─── Worker Entry Point ──────────────────────────────────────────────────────

export default {
    async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
        const url = new URL(request.url);
        const path = url.pathname;

        // CORS Headers (for API endpoints)
        const corsHeaders = {
            'Access-Control-Allow-Origin': '*',
            'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
            'Access-Control-Allow-Headers': 'Content-Type',
        };

        if (request.method === 'OPTIONS') {
            return new Response(null, { headers: corsHeaders });
        }

        // ─── Compliance Pages (required for Google OAuth verification) ────────

        if (request.method === 'GET') {
            if (path === '/' || path === '/index.html') {
                return new Response(htmlPage('Home', HOMEPAGE_BODY), {
                    headers: { 'Content-Type': 'text/html;charset=UTF-8' },
                });
            }
            if (path === '/privacy' || path === '/privacy-policy') {
                return new Response(htmlPage('Privacy Policy', PRIVACY_BODY), {
                    headers: { 'Content-Type': 'text/html;charset=UTF-8' },
                });
            }
            if (path === '/terms' || path === '/terms-of-service') {
                return new Response(htmlPage('Terms of Service', TERMS_BODY), {
                    headers: { 'Content-Type': 'text/html;charset=UTF-8' },
                });
            }
        }

        // ─── API: Exchange authorization code for tokens ─────────────────────

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

                const tokenUrl = 'https://oauth2.googleapis.com/token';
                const params = new URLSearchParams();
                params.append('code', code);
                params.append('client_id', env.GMAIL_CLIENT_ID);
                params.append('client_secret', env.GMAIL_CLIENT_SECRET);
                params.append('redirect_uri', redirect_uri);
                params.append('grant_type', 'authorization_code');

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

        // ─── API: Refresh an expired access token ────────────────────────────

        if (path === '/auth/gmail/refresh' && request.method === 'POST') {
            try {
                const body = await request.json() as any;
                const refresh_token = body.refresh_token;

                if (!refresh_token) {
                    return new Response(JSON.stringify({ error: 'Missing refresh_token' }), {
                        status: 400,
                        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                    });
                }

                const tokenUrl = 'https://oauth2.googleapis.com/token';
                const params = new URLSearchParams();
                params.append('refresh_token', refresh_token);
                params.append('client_id', env.GMAIL_CLIENT_ID);
                params.append('client_secret', env.GMAIL_CLIENT_SECRET);
                params.append('grant_type', 'refresh_token');

                const tokenResponse = await fetch(tokenUrl, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                    body: params
                });

                const tokenData = await tokenResponse.json();

                if (!tokenResponse.ok) {
                    return new Response(JSON.stringify({
                        error: 'Token refresh failed',
                        details: tokenData
                    }), {
                        status: tokenResponse.status,
                        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                    });
                }

                return new Response(JSON.stringify(tokenData), {
                    status: 200,
                    headers: { ...corsHeaders, 'Content-Type': 'application/json' }
                });

            } catch (err: any) {
                console.error('Error in /auth/gmail/refresh:', err);
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
