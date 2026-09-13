import { contextBridge, ipcRenderer } from "electron";
import { IPC, type LanternApi } from "../shared/types";

const api: LanternApi = {
  loadDefaultSong: () => ipcRenderer.invoke(IPC.loadDefaultSong),
  chooseAudioFile: () => ipcRenderer.invoke(IPC.chooseAudioFile),
  saveFile: (suggestedName, bytes) =>
    ipcRenderer.invoke(IPC.saveFile, { suggestedName, bytes }),
  openExternal: (url) => ipcRenderer.invoke(IPC.openExternal, url),
};

contextBridge.exposeInMainWorld("lantern", api);
