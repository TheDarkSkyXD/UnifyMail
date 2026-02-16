import { ComponentRegistry, WorkspaceStore } from 'unifymail-exports';
import UndoRedoToast from './undo-redo-toast';

export function activate() {
  if (AppEnv.isMainWindow()) {
    ComponentRegistry.register(UndoRedoToast, {
      location: WorkspaceStore.Sheet.Global.Footer,
    });
  }
}

export function deactivate() {
  if (AppEnv.isMainWindow()) {
    ComponentRegistry.unregister(UndoRedoToast);
  }
}
