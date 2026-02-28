import crypto from 'crypto';
import { v4 as uuidv4 } from 'uuid';

export const LOCAL_SERVER_PORT = 12141;

export const GMAIL_CLIENT_ID =
  process.env.MS_GMAIL_CLIENT_ID ||
  '400141604862-ceirca79mb14lt7vu06v7ascpo6rj0fr.apps.googleusercontent.com';

export const GMAIL_OAUTH_PROXY_URL = 'https://unifymail-site.leveluptogetherbiz.workers.dev';

export const GMAIL_SCOPES = [
  'https://mail.google.com/', // email
  'https://www.googleapis.com/auth/userinfo.email', // email address
  'https://www.googleapis.com/auth/userinfo.profile', // G+ profile
  'https://www.googleapis.com/auth/contacts', // contacts
];

export const O365_CLIENT_ID =
  process.env.MS_O365_CLIENT_ID || '8787a430-6eee-41e1-b914-681d90d35625';

export const O365_SCOPES = [
  'user.read', // email address
  'offline_access',
  'Contacts.ReadWrite', // contacts
  'Contacts.ReadWrite.Shared', // contacts
  'Calendars.ReadWrite', // calendar
  'Calendars.ReadWrite.Shared', // calendar

  // Future note: When you exchange the refresh token for an access token, you may
  // request these two OR the above set but NOT BOTH, because Microsoft has mapped
  // two underlying systems with different tokens onto the single flow and you
  // need to get an outlook token and not a Micrsosoft Graph token to use these APIs.
  // https://stackoverflow.com/questions/61597263/
  'https://outlook.office.com/IMAP.AccessAsUser.All', // email
  'https://outlook.office.com/SMTP.Send', // email
];

// Re-created only at onboarding page load / auth session start because storing
// verifier would require additional state refactoring
export const CODE_VERIFIER = uuidv4();
export const CODE_CHALLENGE = crypto
  .createHash('sha256')
  .update(CODE_VERIFIER, 'utf8')
  .digest('base64')
  .replace(/\+/g, '-')
  .replace(/\//g, '_')
  .replace(/=/g, '');
