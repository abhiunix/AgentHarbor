import { Outlet } from "react-router-dom";

export function MainContent() {
  return (
    <main className="flex-1 overflow-y-auto p-7">
      <Outlet />
    </main>
  );
}
