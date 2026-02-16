import { MessageViewExtension } from 'unifymail-exports';
import * as ScreenshotMode from './main';

export default class ScreenshotModeMessageExtension extends MessageViewExtension {
    static renderedMessageBodyIntoDocument({ document }) {
        ScreenshotMode.applyToDocument(document);
    }
}
