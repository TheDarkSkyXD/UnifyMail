import { Task } from './task';
import * as Attributes from '../attributes';
import { AttributeValues } from '../models/model';

export class FetchQuotaTask extends Task {
  static attributes = {
    ...Task.attributes,

    supported: Attributes.Boolean({
      modelKey: 'supported',
    }),
    usageKB: Attributes.Number({
      modelKey: 'usageKB',
    }),
    limitKB: Attributes.Number({
      modelKey: 'limitKB',
    }),
  };

  supported: boolean;
  usageKB: number;
  limitKB: number;

  constructor(data: AttributeValues<typeof FetchQuotaTask.attributes> = {}) {
    super(data);
  }
}
