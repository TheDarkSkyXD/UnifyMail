/**
 * mailcore-napi — Node.js N-API bindings for mailcore2
 *
 * Provides in-process access to mailcore2's provider detection,
 * account validation, and connection testing from Electron/Node.js.
 */

export interface NetServiceInfo {
  hostname: string;
  port: number;
  connectionType: 'tls' | 'starttls' | 'clear';
}

export interface MailProviderInfo {
  identifier: string;
  servers: {
    imap: NetServiceInfo[];
    smtp: NetServiceInfo[];
    pop: NetServiceInfo[];
  };
  domainMatch: string[];
  mxMatch: string[];
}

export interface AccountValidationResult {
  success: boolean;
  error?: string;
  identifier?: string;
  imapServer?: { hostname: string; port: number };
  smtpServer?: { hostname: string; port: number };
}

export interface IMAPConnectionResult {
  success: boolean;
  error?: string;
  capabilities?: string[];
}

export interface SMTPConnectionResult {
  success: boolean;
  error?: string;
}

/**
 * Look up a mail provider by email address (sync — fast in-memory lookup).
 * Uses mailcore2's MailProvidersManager which matches against domain and MX records.
 */
export function providerForEmail(email: string): MailProviderInfo | null;

/**
 * Load providers from a custom providers.json file.
 * Called automatically on module load with the bundled providers.json.
 */
export function registerProviders(jsonPath: string): void;

/**
 * Validate an email account (async — network I/O on worker thread).
 * Tests IMAP and SMTP connectivity using mailcore2's AccountValidator.
 */
export function validateAccount(opts: {
  email: string;
  password?: string;
  oauth2Token?: string;
  imapHostname?: string;
  imapPort?: number;
  smtpHostname?: string;
  smtpPort?: number;
}): Promise<AccountValidationResult>;

/**
 * Test an IMAP connection (async — network I/O on worker thread).
 * Returns connection success and server capabilities.
 */
export function testIMAPConnection(opts: {
  hostname: string;
  port: number;
  connectionType?: 'tls' | 'starttls' | 'clear';
  username?: string;
  password?: string;
  oauth2Token?: string;
}): Promise<IMAPConnectionResult>;

/**
 * Test an SMTP connection (async — network I/O on worker thread).
 */
export function testSMTPConnection(opts: {
  hostname: string;
  port: number;
  connectionType?: 'tls' | 'starttls' | 'clear';
  username?: string;
  password?: string;
  oauth2Token?: string;
}): Promise<SMTPConnectionResult>;
