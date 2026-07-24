## 2026-07-03T21:55:23Z

You are the Worker for Milestone 5: forge-desktop Dashboard (R6).
Your working directory is `/home/arthur/projects/forge-core/.agents/worker_forge_desktop_m5`.

Please perform the following tasks:
1. Create the project directory `/home/arthur/projects/forge-desktop/`.
2. Write the following configuration files:
   - `/home/arthur/projects/forge-desktop/package.json`
   - `/home/arthur/projects/forge-desktop/tsconfig.json`
   - `/home/arthur/projects/forge-desktop/vite.config.ts`
   - `/home/arthur/projects/forge-desktop/index.html`
3. Write Electron files:
   - `/home/arthur/projects/forge-desktop/main.js` (main process executing `./target/release/forge list --output json` and exposing it via ipcMain)
   - `/home/arthur/projects/forge-desktop/preload.js` (preload script exposing it safely via contextBridge)
4. Write React files:
   - `/home/arthur/projects/forge-desktop/src/main.tsx`
   - `/home/arthur/projects/forge-desktop/src/index.css` (implementing glassmorphism, dark mode, smooth animations)
   - `/home/arthur/projects/forge-desktop/src/App.tsx` (exposing visual cards with workflows, goals, status, and task completion progress)
5. Initialize the project:
   - Run `npm install` inside `/home/arthur/projects/forge-desktop` to install all dependencies.
   - Run `npm run build` inside `/home/arthur/projects/forge-desktop` to verify that React + TypeScript + Vite builds without any compilation errors.
6. Write a handoff.md in your working directory and send a message back to the parent (conversation ID: 3e9f825f-a52f-4f9b-8826-e0ccd6f322a6) with the path to your handoff.md.

MANDATORY INTEGRITY WARNING: DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
