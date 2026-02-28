import Config from '../../unifymail-frontend/src/config';

export default class ConfigMigrator {
  config: Config;

  constructor(config) {
    this.config = config;
  }

  migrate() {}
}
