import React from 'react';
import {
  localized,
  PropTypes,
  APIError,
  UnifyMailAPIRequest,
  Message,
  DraftEditingSession,
} from 'unifymail-exports';
import { MetadataComposerToggleButton } from 'unifymail-component-kit';
import { PLUGIN_ID, PLUGIN_NAME } from './link-tracking-constants';

export default class LinkTrackingButton extends React.Component<{
  draft: Message;
  session: DraftEditingSession;
}> {
  static displayName = 'LinkTrackingButton';

  static containersRequired: false;

  static propTypes = {
    draft: PropTypes.object.isRequired,
    session: PropTypes.object.isRequired,
  };

  shouldComponentUpdate(nextProps) {
    return (
      nextProps.draft.metadataForPluginId(PLUGIN_ID) !==
      this.props.draft.metadataForPluginId(PLUGIN_ID)
    );
  }

  _errorMessage(error) {
    if (
      error instanceof APIError &&
      UnifyMailAPIRequest.TimeoutErrorCodes.includes(error.statusCode)
    ) {
      return localized(
        `Link tracking does not work offline. Please re-enable when you come back online.`
      );
    }
    return localized(
      `Unfortunately, link tracking servers are currently not available. Please try again later. Error: %@`,
      error.message
    );
  }

  render() {
    if (this.props.draft.plaintext) {
      return <span />;
    }

    return (
      <MetadataComposerToggleButton
        iconName="icon-composer-linktracking.png"
        pluginId={PLUGIN_ID}
        pluginName={PLUGIN_NAME}
        metadataEnabledValue={{ tracked: true }}
        errorMessage={this._errorMessage}
        draft={this.props.draft}
        session={this.props.session}
      />
    );
  }
}
