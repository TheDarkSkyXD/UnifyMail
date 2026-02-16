import React from 'react';
import { localized, Actions, AccountStore, IdentityStore, IIdentity } from 'unifymail-exports';
import { Notification } from 'unifymail-component-kit';

export default class PleaseSubscribeNotification extends React.Component<
  Record<string, unknown>,
  { msg: string }
> {
  static displayName = 'PleaseSubscribeNotification';

  unlisteners: Array<() => void>;

  constructor(props) {
    super(props);
    this.state = this.getStateFromStores();
  }

  componentDidMount() {
    this.unlisteners = [
      AccountStore.listen(() => this.setState(this.getStateFromStores())),
      IdentityStore.listen(() => this.setState(this.getStateFromStores())),
    ];
  }

  componentWillUnmount() {
    for (const u of this.unlisteners) {
      u();
    }
  }

  getStateFromStores() {
    const { stripePlanEffective, stripePlan } = (IdentityStore.identity() || {}) as IIdentity;
    const accountCount = AccountStore.accounts().length;

    let msg = null;
    if (stripePlan === 'Basic' && accountCount > 4) {
      msg = localized(`Please consider paying for UnifyMail Pro!`);
    }
    if (stripePlan !== stripePlanEffective) {
      msg = localized(`We're having trouble billing your UnifyMail subscription.`);
    }

    return { msg };
  }

  render() {
    if (!this.state.msg) {
      return <span />;
    }
    return (
      <Notification
        priority="0"
        isDismissable={true}
        title={this.state.msg}
        actions={[
          {
            label: localized('Manage'),
            fn: () => {
              Actions.switchPreferencesTab('Subscription');
              Actions.openPreferences();
            },
          },
        ]}
      />
    );
  }
}
