import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { chooseAudioFile, loadDefaultSong, loadSongFolder, saveFile } from "./assetLoader";
import { IPC, type SaveFileRequest } from "../shared/types";

const rendererUrl = process.env.ELECTRON_RENDERER_URL;

function createWindow(): void {
  const win = new BrowserWindow({
    width: 1440,
    height: 940,
    minWidth: 900,
    minHeight: 640,
    backgroundColor: "#101012",
    title: "Lantern Music Player",
    webPreferences: {
      preload: path.join(__dirname, "../preload/index.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (rendererUrl) {
    void win.loadURL(rendererUrl);
  } else {
    void win.loadFile(path.join(__dirname, "../renderer/index.html"));
  }
}

app.whenReady().then(() => {
  ipcMain.handle(IPC.loadDefaultSong, () => loadDefaultSong());
  ipcMain.handle(IPC.loadSongFolder, () => loadSongFolder());
  ipcMain.handle(IPC.chooseAudioFile, () => chooseAudioFile());
  ipcMain.handle(IPC.saveFile, (_event, request: SaveFileRequest) =>
    saveFile(request.suggestedName, request.bytes),
  );

  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
