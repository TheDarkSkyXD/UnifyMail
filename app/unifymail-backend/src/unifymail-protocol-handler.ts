import { protocol } from 'electron';
import fs from 'fs';
import path from 'path';

// Handles requests with 'UnifyMail' protocol.
//
// It's created by {Application} upon instantiation and is used to create a
// custom resource loader for 'UnifyMail://' URLs.
//
// The following directories are searched in order:
//   * <config-dir>/assets
//   * <config-dir>/dev/packages (unless in safe mode)
//   * <config-dir>/packages
//   * RESOURCE_PATH/node_modules
//
export default class UnifyMailProtocolHandler {
  loadPaths: string[] = [];

  constructor({ configDirPath, resourcePath, safeMode }) {
    if (!safeMode) {
      this.loadPaths.push(path.join(configDirPath, 'dev', 'packages'));
    }
    this.loadPaths.push(path.join(configDirPath, 'packages'));
    this.loadPaths.push(path.join(resourcePath, 'internal_packages'));

    this.registerProtocol();
  }

  // Creates the 'UnifyMail' custom protocol handler.
  registerProtocol() {
    const scheme = 'UnifyMail';
    protocol.registerFileProtocol(scheme, (request, callback) => {
      const relativePath = path.normalize(request.url.substr(scheme.length + 1));

      let filePath = null;
      for (const loadPath of this.loadPaths) {
        filePath = path.join(loadPath, relativePath);
        const fileStats = fs.statSyncNoException(filePath);
        if (fileStats.isFile && fileStats.isFile()) {
          break;
        }
      }

      callback(filePath);
    });
  }
}
