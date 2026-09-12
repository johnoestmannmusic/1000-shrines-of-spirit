import { createContext, useContext } from "react";

export interface ExplainerContent {
  title: string;
  body: string;
}

export const DEFAULT_EXPLAINER: ExplainerContent = {
  title: "EXPLAINER",
  body: "Hover a card title or the cover art to see what it represents. Pattern cells and instrument controls also provide precise hover details.",
};

export const ExplainerContext = createContext<(content: ExplainerContent) => void>(() => {});
export function useExplainer(): (content: ExplainerContent) => void {
  return useContext(ExplainerContext);
}
