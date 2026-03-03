import Config from '../frontend/config';

export default class ConfigMigrator {
  config: Config;

  constructor(config) {
    this.config = config;
  }

  migrate() {}
}
