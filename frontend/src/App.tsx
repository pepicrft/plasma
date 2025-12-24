import { useSearchParams } from "react-router-dom"
import { GetStarted } from "@/components/GetStarted"
import { MainLayout } from "@/components/MainLayout"

function App() {
  const [searchParams] = useSearchParams()
  const projectPath = searchParams.get("project")

  if (!projectPath) {
    return <GetStarted />
  }

  return <MainLayout projectPath={projectPath} />
}

export default App
