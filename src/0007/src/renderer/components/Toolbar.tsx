import { useExplainer } from "../explainer";
import { downloadWavExplain, projectJsonExplain } from "../explainerContent";

interface ToolbarProps {
  onNewProject: () => void;
  onSaveFur: () => void;
  onSaveMidi: () => void;
  onOpenProjectJson: () => void;
  onSaveWav: () => void;
  wavReady: boolean;
  furReady: boolean;
  status: string;
}

export function Toolbar(props: ToolbarProps) {
  const explain = useExplainer();
  return (
    <section className="panel toolbar">
      <div className="row wrap">
        <button
          onClick={props.onSaveFur}
          disabled={!props.furReady}
          title={props.furReady ? "Save the loaded Furnace module" : "No .fur is loaded — this is a project-only song"}
        >
          Save .FUR
        </button>
        <button onClick={props.onSaveMidi}>Save .MIDI</button>
        <button
          onClick={props.onSaveWav}
          onMouseEnter={() => explain(downloadWavExplain())}
          disabled={!props.wavReady}
        >
          Save .WAV
        </button>
        <button onClick={props.onOpenProjectJson} onMouseEnter={() => explain(projectJsonExplain())}>
          Project JSON
        </button>
        <button onClick={props.onNewProject} title="Clear patterns, reset to one default instrument">
          New Project
        </button>
        <span className="spacer" />
        <span className="mono status">{props.status}</span>
      </div>
    </section>
  );
}
