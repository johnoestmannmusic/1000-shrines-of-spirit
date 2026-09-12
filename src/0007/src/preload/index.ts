import { contextBridge, ipcRenderer } from "electron";
import { IPC, type LanternApi } from "../shared/types";

const api: LanternApi = {
  loadDefaultSong: () => ipcRenderer.invoke(IPC.loadDefaultSong),
  loadSongFolder: () => ipcRenderer.invoke(IPC.loadSongFolder),
  chooseAudioFile: () => ipcRenderer.invoke(IPC.chooseAudioFile),
  saveFile: (suggestedName, bytes) =>
    ipcRenderer.invoke(IPC.saveFile, { suggestedName, bytes }),
};

contextBridge.exposeInMainWorld("lantern", api);
