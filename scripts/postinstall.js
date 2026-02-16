#!/usr/bin/env node
/* eslint global-require: 0 */
/* eslint quote-props: 0 */
const path = require('path');
const https = require('https');
const fs = require('fs');
const rimraf = require('rimraf');
const targz = require('targz');
const { safeExec } = require('./utils/child-process-wrapper.js');
const { execSync } = require('child_process');

const appDependencies = require('../app/package.json').dependencies;
const rootDependencies = require('../package.json').dependencies;
const npmElectronTarget = rootDependencies.electron;
const npmEnvs = {
  system: process.env,
  electron: Object.assign({}, process.env, {
    npm_config_target: npmElectronTarget,
    npm_config_arch: process.env.OVERRIDE_TO_INTEL ? 'x64' : process.arch,
    npm_config_target_arch: process.env.OVERRIDE_TO_INTEL ? 'x64' : process.arch,
    npm_config_disturl: 'https://electronjs.org/headers',
    npm_config_runtime: 'electron',
  }),
};

function npm(cmd, options) {
  const { cwd, env } = Object.assign({ cwd: '.', env: 'system' }, options);

  return new Promise((resolve, reject) => {
    console.log(
      `\n-- Running npm ${cmd} in ${cwd} with ${env} config (arch=${npmEnvs[env].npm_config_target_arch}) --`
    );

    safeExec(
      `npm ${cmd}`,
      {
        cwd: path.resolve(__dirname, '..', cwd),
        env: npmEnvs[env],
      },
      (err, stdout) => {
        return err ? reject(err) : resolve(stdout);
      }
    );
  });
}



// For speed, we cache app/node_modules. However, we need to
// be sure to do a full rebuild of native node modules when the
// Electron version changes. To do this we check a marker file.
const appPath = path.resolve(__dirname, '..', 'app');
const appModulesPath = path.resolve(appPath, 'node_modules');
const cacheVersionPath = path.join(appModulesPath, '.postinstall-target-version');
const cacheElectronTarget =
  fs.existsSync(cacheVersionPath) && fs.readFileSync(cacheVersionPath).toString();

if (cacheElectronTarget !== npmElectronTarget) {
  console.log(
    `\n-- Clearing app/node_modules (${cacheElectronTarget} !== ${npmElectronTarget}) --`
  );
  rimraf.sync(appModulesPath);
}

// Audit is emitted with npm ls, no need to run it on EVERY command which is an odd default

async function sqliteMissingNanosleep() {
  return new Promise(resolve => {
    const sqliteLibDir = path.join(appModulesPath, 'better-sqlite3', 'build', 'Release');
    const staticLib = path.join(sqliteLibDir, 'sqlite3.a');
    const sharedLib = path.join(sqliteLibDir, 'better_sqlite3.node');

    // Check the static lib first (build-from-source), then the prebuilt .node binary
    const target = fs.existsSync(staticLib) ? staticLib : sharedLib;
    safeExec(
      `nm '${target}' | grep nanosleep`,
      { ignoreStderr: true },
      (err, resp) => {
        resolve(resp === '');
      }
    );
  });
}

async function run() {
  // run `npm install` in ./app with Electron NPM config
  await npm(`install --no-audit`, { cwd: './app', env: 'electron' });

  // run `npm dedupe` in ./app with Electron NPM config
  await npm(`dedupe --no-audit`, { cwd: './app', env: 'electron' });

  // run `npm ls` in ./app - detects missing peer dependencies, etc.
  await npm(`ls`, { cwd: './app', env: 'electron' });

  // if SQlite was not built with HAVE_NANOSLEEP, do not ship this build! We need nanosleep
  // support so that multiple processes can connect to the sqlite file at the same time.
  // Without it, transactions only retry every 1 sec instead of every 10ms, leading to
  // awful db lock contention.  https://github.com/WiseLibs/better-sqlite3/issues/597
  if (['linux', 'darwin'].includes(process.platform) && (await sqliteMissingNanosleep())) {
    console.error(`better-sqlite compiled without -HAVE_NANOSLEEP, do not ship this build!`);
    process.exit(1001);
  }

  // write the marker with the electron version
  fs.writeFileSync(cacheVersionPath, npmElectronTarget);

  // if the user hasn't cloned the mailsync module, alert them!
  const mailsyncParams = process.platform === 'win32'
    ? { exe: 'mailsync.exe', cmd: 'Open mailsync/Windows/mailsync.sln in Visual Studio, build Release configuration, and copy the output exe to app/mailsync.exe' }
    : { exe: 'mailsync', cmd: 'cd mailsync && make && cp mailsync ../app/' };

  if (!fs.existsSync(path.join(appPath, mailsyncParams.exe))) {
    console.error(
      `\n---------------------------------------------------------------\n` +
      `⚠️  ACTION REQUIRED: BUILD MAILSYNC ⚠️\n` +
      `---------------------------------------------------------------\n` +
      `We no longer distribute pre-built binaries via S3.\n` +
      `You must build 'mailsync' from source and place it in the 'app' folder.\n\n` +
      `INSTRUCTIONS:\n` +
      `1. Initialize submodule: git submodule update --init --recursive\n` +
      `2. Build it:\n` +
      `   ${mailsyncParams.cmd}\n` +
      `---------------------------------------------------------------\n`
    );
  } else {
    console.log(
      `\n-- Mailsync binary detected in ./app. Good to go! --`
    );
  }
}

run();
