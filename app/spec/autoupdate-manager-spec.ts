import AutoUpdateManager from '../unifymail-backend/src/autoupdate-manager';

describe('AutoUpdateManager', function() {
  beforeEach(function() {
    this.UnifyMailIdentityId = null;
    this.specMode = true;
    this.config = {
      set: jasmine.createSpy('config.set'),
      get: key => {
        if (key === 'identity.id') {
          return this.UnifyMailIdentityId;
        }
        if (key === 'env') {
          return 'production';
        }
      },
      onDidChange: (key, callback) => {
        return callback();
      },
    };
  });

  describe('with attached commit version', () =>
    it('correctly sets the feedURL', function() {
      const m = new AutoUpdateManager('3.222.1-abc', this.config, this.specMode);
      spyOn(m, 'setupAutoUpdater');
      expect(m.feedURL).toEqual(
        'https://github.com/TheDarkSkyXD/UnifyMail/releases' +
          process.platform +
          '/' +
          process.arch +
          '/3.222.1-abc/anonymous/stable'
      );
    }));

  describe('with no attached commit', () =>
    it('correctly sets the feedURL', function() {
      const m = new AutoUpdateManager('3.222.1', this.config, this.specMode);
      spyOn(m, 'setupAutoUpdater');
      expect(m.feedURL).toEqual(
        'https://github.com/TheDarkSkyXD/UnifyMail/releases' +
          process.platform +
          '/' +
          process.arch +
          '/3.222.1/anonymous/stable'
      );
    }));

  describe('when an update identity is already set', () =>
    it('should send it and not save any changes', function() {
      this.UnifyMailIdentityId = 'test-UnifyMail-id';
      const m = new AutoUpdateManager('3.222.1', this.config, this.specMode);
      expect(m.feedURL).toEqual(
        'https://github.com/TheDarkSkyXD/UnifyMail/releases' +
          process.platform +
          '/' +
          process.arch +
          '/3.222.1/test-UnifyMail-id/stable'
      );
    }));

  describe('when an update identity is added', () =>
    it('should update the feed URL', function() {
      const m = new AutoUpdateManager('3.222.1', this.config, this.specMode);
      spyOn(m, 'setupAutoUpdater');
      expect(m.feedURL.includes('anonymous')).toEqual(true);
      this.UnifyMailIdentityId = 'test-UnifyMail-id';
      m.updateFeedURL();
      expect(m.feedURL.includes(this.UnifyMailIdentityId)).toEqual(true);
    }));
});
