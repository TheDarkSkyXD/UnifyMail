import { SoundRegistry } from 'unifymail-exports';

export function activate() {
  SoundRegistry.register('send', 'UnifyMail://custom-sounds/CUSTOM_UI_Send_v1.ogg');
  SoundRegistry.register('confirm', 'UnifyMail://custom-sounds/CUSTOM_UI_Confirm_v1.ogg');
  SoundRegistry.register('hit-send', 'UnifyMail://custom-sounds/CUSTOM_UI_HitSend_v1.ogg');
  SoundRegistry.register('new-mail', 'UnifyMail://custom-sounds/CUSTOM_UI_NewMail_v1.ogg');
}

export function deactivate() {
  SoundRegistry.unregister(['send', 'confirm', 'hit-send', 'new-mail']);
}
