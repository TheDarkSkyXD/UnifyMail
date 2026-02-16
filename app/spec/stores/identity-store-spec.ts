import { Utils, KeyManager } from 'unifymail-exports';
import { IdentityStore } from '../../src/flux/stores/identity-store';
import * as UnifyMailAPIRequest from '../../src/flux/unifymail-api-request';

const TEST_NYLAS_ID = 'icihsnqh4pwujyqihlrj70vh';

describe('IdentityStore', function identityStoreSpec() {
  beforeEach(() => {
    this.identityJSON = {
      firstName: 'UnifyMail 050',
      lastName: 'Test',
      email: 'UnifyMail050test@evanmorikawa.com',
      id: TEST_NYLAS_ID,
      featureUsage: {
        feat: {
          quota: 10,
          usedInPeriod: 1,
        },
      },
      token: 'secret token',
    };
  });

  describe('saveIdentity', () => {
    beforeEach(() => {
      IdentityStore._identity = this.identityJSON;
      spyOn(KeyManager, 'deletePassword');
      spyOn(KeyManager, 'replacePassword');
      spyOn(IdentityStore, 'trigger');
      spyOn(AppEnv.config, 'set');
      spyOn(AppEnv.config, 'unset');
    });

    it('clears passwords if unsetting', async () => {
      await IdentityStore.saveIdentity(null);
      expect(KeyManager.deletePassword).toHaveBeenCalled();
      expect(KeyManager.replacePassword).not.toHaveBeenCalled();
      expect(AppEnv.config.set).toHaveBeenCalled();
      const ident = AppEnv.config.set.calls[0].args[1];
      expect(ident).toBe(null);
    });

    it('applies changes synchronously', async () => {
      const used = () => IdentityStore.identity().featureUsage.feat.usedInPeriod;
      expect(used()).toBe(1);

      const next = JSON.parse(JSON.stringify(this.identityJSON));
      next.featureUsage.feat.usedInPeriod += 1;
      await IdentityStore.saveIdentity(next);
      expect(used()).toBe(2);
    });
  });

  describe('returning the identity object', () => {
    beforeEach(() => {
      spyOn(IdentityStore, 'saveIdentity').andReturn(Promise.resolve());
    });
    it('returns the identity as null if it looks blank', () => {
      IdentityStore._identity = null;
      expect(IdentityStore.identity()).toBe(null);
      IdentityStore._identity = {};
      expect(IdentityStore.identity()).toBe(null);
      IdentityStore._identity = { token: 'bad' };
      expect(IdentityStore.identity()).toBe(null);
    });

    it('returns a proper clone of the identity', () => {
      IdentityStore._identity = { id: 'bar', deep: { obj: 'baz' } };
      const ident = IdentityStore.identity();
      IdentityStore._identity.deep.obj = 'changed';
      expect(ident.deep.obj).toBe('baz');
    });
  });

  describe('fetchIdentity', () => {
    beforeEach(() => {
      IdentityStore._identity = this.identityJSON;
      spyOn(IdentityStore, 'saveIdentity');
      spyOn(AppEnv, 'reportError');
      spyOn(console, 'error');
    });

    it('saves the identity returned', async () => {
      const resp = Utils.deepClone(this.identityJSON);
      resp.featureUsage.feat.quota = 5;
      spyOn(UnifyMailAPIRequest, 'makeRequest').andCallFake(() => {
        return Promise.resolve(resp);
      });
      await IdentityStore.fetchIdentity();
      expect(UnifyMailAPIRequest.makeRequest).toHaveBeenCalled();
      const options = UnifyMailAPIRequest.makeRequest.calls[0].args[0];
      expect(options.path).toEqual('/api/me');
      expect(IdentityStore.saveIdentity).toHaveBeenCalled();
      const newIdent = IdentityStore.saveIdentity.calls[0].args[0];
      expect(newIdent.featureUsage.feat.quota).toBe(5);
      expect(AppEnv.reportError).not.toHaveBeenCalled();
    });

    it('errors if the json is invalid', async () => {
      spyOn(UnifyMailAPIRequest, 'makeRequest').andCallFake(() => {
        return Promise.resolve({});
      });
      await IdentityStore.fetchIdentity();
      expect(AppEnv.reportError).toHaveBeenCalled();
      expect(IdentityStore.saveIdentity).not.toHaveBeenCalled();
    });
  });
});
