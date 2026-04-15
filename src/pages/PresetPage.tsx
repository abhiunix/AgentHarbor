import { useLocation } from "react-router-dom";
import { PresetList } from "../components/presets/PresetList";
import { PresetView } from "../components/presets/PresetView";

export function PresetPage() {
  const location = useLocation();
  const pathAfterPresets = location.pathname.replace(/^\/presets\/?/, "");
  const presetId = pathAfterPresets.split("/")[0];

  if (!presetId) {
    return <PresetList />;
  }
  return <PresetView />;
}
