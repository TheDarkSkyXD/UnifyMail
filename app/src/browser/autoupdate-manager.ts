/* eslint global-require: 0*/
import { dialog, nativeImage } from 'electron';
import { EventEmitter } from 'events';
import path from 'path';
import fs from 'fs';
import { localized } from '../intl';
import { autoUpdater } from 'electron-updater';
import log from 'electron-log';

// Configure logging
autoUpdater.logger = log;
(autoUpdater.logger as any).transports.file.level = 'info';

const IdleState = 'idle';
const CheckingState = 'checking';
const DownloadingState = 'downloading';
const UpdateAvailableState = 'update-available';
const NoUpdateAvailableState = 'no-update-available';
const UnsupportedState = 'unsupported';
const ErrorState = 'error';

export default class AutoUpdateManager extends EventEmitter {
  state = IdleState;
  version: string;
  config: import('../config').default;
  specMode: boolean;
  releaseNotes: string;
  releaseVersion: string;

  constructor(version, config, specMode) {
    super();

    this.version = version;
    this.config = config;
    this.specMode = specMode;

    // Set GitHub repository explicitly
    autoUpdater.setFeedURL({
      provider: 'github',
      owner: 'TheDarkSkyXD',
      repo: 'UnifyMail',
    });

    setTimeout(() => this.setupAutoUpdater(), 0);
  }

  setupAutoUpdater() {
    autoUpdater.on('error', error => {
      if (this.specMode) return;
      console.error(`Error Downloading Update: ${error.message}`);
      this.setState(ErrorState);
    });

    autoUpdater.on('checking-for-update', () => {
      this.setState(CheckingState);
    });

    autoUpdater.on('update-not-available', () => {
      this.setState(NoUpdateAvailableState);
    });

    autoUpdater.on('update-available', () => {
      this.setState(DownloadingState);
    });

    autoUpdater.on('update-downloaded', (info) => {
      this.releaseNotes = typeof info.releaseNotes === 'string' ? info.releaseNotes : (info.releaseNotes || []).map(n => n.note).join('\n');
      this.releaseVersion = info.version;
      this.setState(UpdateAvailableState);
      this.emitUpdateAvailableEvent();
    });

    // Check immediately at startup
    this.check({ hidePopups: true });

    // Check every 30 minutes
    setInterval(() => {
      if ([UpdateAvailableState, UnsupportedState].includes(this.state)) {
        console.log('Skipping update check... update ready to install, or updater unavailable.');
        return;
      }
      this.check({ hidePopups: true });
    }, 1000 * 60 * 30);
  }

  emitUpdateAvailableEvent() {
    if (!this.releaseVersion) {
      return;
    }
    // Check if windowManager exists (might be undefined in tests/early startup)
    const windowManager = global.application?.windowManager;
    if (windowManager) {
      windowManager.sendToAllWindows(
        'update-available',
        {},
        this.getReleaseDetails()
      );
    }
  }

  setState(state) {
    if (this.state === state) {
      return;
    }
    this.state = state;
    this.emit('state-changed', this.state);
  }

  getState() {
    return this.state;
  }

  getReleaseDetails() {
    return {
      releaseVersion: this.releaseVersion,
      releaseNotes: this.releaseNotes,
    };
  }

  check({ hidePopups }: { hidePopups?: boolean } = {}) {
    if (!hidePopups) {
      autoUpdater.once('update-not-available', this.onUpdateNotAvailable);
      autoUpdater.once('error', this.onUpdateError);
    }
    autoUpdater.checkForUpdates();
  }

  install() {
    autoUpdater.quitAndInstall();
  }

  dialogIcon() {
    if (!global.application || !global.application.resourcePath) return undefined;
    const iconPath = path.join(
      global.application.resourcePath,
      'static',
      'images',
      'UnifyMail.png'
    );
    if (!fs.existsSync(iconPath)) return undefined;
    return nativeImage.createFromPath(iconPath);
  }

  onUpdateNotAvailable = () => {
    // Remove listeners to avoid accumulation if check is called multiple times
    // (though 'once' handles it, explicit removal in error ensures cleanup)
    autoUpdater.removeListener('error', this.onUpdateError);

    dialog.showMessageBox({
      type: 'info',
      buttons: [localized('OK')],
      icon: this.dialogIcon(),
      message: localized('No update available.'),
      title: localized('No update available.'),
      detail: localized(`You're running the latest version of UnifyMail (%@).`, this.version),
    });
  };

  onUpdateError = (event: any, message?: string) => { // Adapted signature
    autoUpdater.removeListener('update-not-available', this.onUpdateNotAvailable);

    // electron-updater error event passes Error object as first argument
    const errorMsg = (event instanceof Error) ? event.message : (message || String(event));

    dialog.showMessageBox({
      type: 'warning',
      buttons: [localized('OK')],
      icon: this.dialogIcon(),
      message: localized('There was an error checking for updates.'),
      title: localized('Update Error'),
      detail: errorMsg,
    });
  };
}
