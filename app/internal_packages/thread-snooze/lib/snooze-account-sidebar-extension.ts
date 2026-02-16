import { localized, MailboxPerspective } from 'unifymail-exports';

export const name = 'SnoozeAccountSidebarExtension';

export function sidebarItem(accountIds: string[]) {
  return {
    id: 'snoozed',
    name: localized('Snoozed'),
    iconName: 'snoozed.png',
    perspective: MailboxPerspective.forStandardCategories(accountIds, 'snoozed'),
    insertAtTop: true,
  };
}
