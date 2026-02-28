import { Task } from './task';
import * as Attributes from '../attributes';
import { AttributeValues } from '../models/model';

export class SearchMessagesTask extends Task {
  static attributes = {
    ...Task.attributes,

    query: Attributes.String({
      modelKey: 'query',
    }),
    folderId: Attributes.String({
      modelKey: 'folderId',
    }),
    resultUIDs: Attributes.Obj({
      modelKey: 'resultUIDs',
    }),
    resultCount: Attributes.Number({
      modelKey: 'resultCount',
    }),
  };

  query: string;
  folderId: string;
  resultUIDs: number[];
  resultCount: number;

  constructor(data: AttributeValues<typeof SearchMessagesTask.attributes> = {}) {
    super(data);
  }
}
