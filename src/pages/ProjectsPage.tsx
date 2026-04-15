import { useState, useEffect } from "react";
import { useLocation } from "react-router-dom";
import { ProjectList } from "../components/projects/ProjectList";
import { ProjectDetail } from "../components/projects/ProjectDetail";
import { DeployWizard } from "../components/deploy/DeployWizard";

export function ProjectsPage() {
  const location = useLocation();
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [deployProject, setDeployProject] = useState<string | null>(null);

  useEffect(() => {
    const path = (location.state as { projectPath?: string } | null)?.projectPath;
    if (path) setSelectedProject(path);
  }, [location.state]);

  const handleSelectProject = (path: string) => {
    setSelectedProject(path || null);
  };

  const handleCloseDetail = () => {
    setSelectedProject(null);
  };

  const handleRedeploy = (projectPath: string) => {
    setDeployProject(projectPath);
  };

  const handleCloseDeployWizard = () => {
    setDeployProject(null);
  };

  return (
    <div className="h-full flex">
      <div className="flex-1 min-w-0">
        <ProjectList
          onSelectProject={handleSelectProject}
          selectedPath={selectedProject}
        />
      </div>

      {selectedProject && (
        <ProjectDetail
          projectPath={selectedProject}
          onClose={handleCloseDetail}
          onRedeploy={handleRedeploy}
        />
      )}

      {deployProject && (
        <DeployWizard
          isOpen={true}
          onClose={handleCloseDeployWizard}
          initialProjectPath={deployProject}
        />
      )}
    </div>
  );
}
