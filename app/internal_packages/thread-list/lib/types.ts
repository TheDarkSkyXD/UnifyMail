import { Message, Thread } from 'unifymail-exports';

export interface ThreadWithMessagesMetadata extends Thread {
  __messages: Message[];
}
