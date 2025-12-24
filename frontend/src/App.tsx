import { useEffect } from "react"
import { useSearchParams } from "react-router-dom"
import { GetStarted } from "@/components/GetStarted"
import { MainLayout } from "@/components/MainLayout"
import { checkForUpdates } from "@/lib/updater"

function App() {
  const [searchParams] = useSearchParams()
  const projectPath = searchParams.get("project")

  // Check for updates on app startup (only in Tauri environment)
  useEffect(() => {
    // Only run updater in Tauri environment, not in tests or browser
    if (typeof window !== 'undefined' && '__TAURI__' in window) {
      checkForUpdates()
    }
  }, [])

  if (!projectPath) {
    return <GetStarted />
  }

  return <MainLayout projectPath={projectPath} />
}

export default App
