import { localized, ComponentRegistry, ExtensionRegistry } from 'unifymail-exports';
import { HasTutorialTip } from 'unifymail-component-kit';
import OpenTrackingButton from './open-tracking-button';
import OpenTrackingIcon from './open-tracking-icon';
import OpenTrackingMessageStatus from './open-tracking-message-status';
import OpenTrackingComposerExtension from './open-tracking-composer-extension';

const OpenTrackingButtonWithTutorialTip = HasTutorialTip(OpenTrackingButton, {
  title: localized('See when recipients open this email'),
  instructions: localized(
    'When enabled, UnifyMail will notify you as soon as someone reads this message. Sending to a group? UnifyMail shows you which recipients opened your email so you can follow up with precision.'
  ),
});

export function activate() {
  ComponentRegistry.register(OpenTrackingButtonWithTutorialTip, { role: 'Composer:ActionButton' });

  ComponentRegistry.register(OpenTrackingIcon, { role: 'ThreadListIcon' });

  ComponentRegistry.register(OpenTrackingMessageStatus, { role: 'MessageHeaderStatus' });

  ExtensionRegistry.Composer.register(OpenTrackingComposerExtension);
}

export function serialize() {}

export function deactivate() {
  ComponentRegistry.unregister(OpenTrackingButtonWithTutorialTip);
  ComponentRegistry.unregister(OpenTrackingIcon);
  ComponentRegistry.unregister(OpenTrackingMessageStatus);
  ExtensionRegistry.Composer.unregister(OpenTrackingComposerExtension);
}
