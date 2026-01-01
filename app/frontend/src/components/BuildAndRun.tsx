import { useState, useEffect, useCallback } from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Play, FolderOpen, Loader2, CheckCircle, XCircle, ChevronDown, ChevronUp, Terminal } from "lucide-react"
import { StreamViewer } from "@/components/StreamViewer"

interface Simulator {
  udid: string
  name: string
  state: string
  runtime: string
}

// Schemes are returned as simple strings from the API
type Scheme = string

interface BuildProduct {
  name: string
  path: string
}

type BuildState =
  | { status: "idle" }
  | { status: "building"; lines: string[] }
  | { status: "installing" }
  | { status: "streaming"; udid: string }
  | { status: "error"; message: string }
  | { status: "success"; products: BuildProduct[] }

interface StreamLogEvent {
  type: "info" | "error" | "debug" | "frame"
  message?: string
  frame_number?: number
}

export function BuildAndRun() {
  const [projectPath, setProjectPath] = useState("/Users/pepicrft/src/github.com/pepicrft/Plasma/app/tests/fixtures/xcode")
  const [simulators, setSimulators] = useState<Simulator[]>([])
  const [selectedSimulator, setSelectedSimulator] = useState("")
  const [schemes, setSchemes] = useState<Scheme[]>([])
  const [selectedScheme, setSelectedScheme] = useState("")
  const [buildState, setBuildState] = useState<BuildState>({ status: "idle" })
  const [streamUrl, setStreamUrl] = useState<string | null>(null)
  const [streamLogs, setStreamLogs] = useState<string[]>([])
  const [showLogs, setShowLogs] = useState(true)

  // Discover schemes for a given path
  const discoverSchemes = useCallback(async (path: string) => {
    if (!path) return

    try {
      const res = await fetch("/api/xcode/discover", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path }),
      })

      if (!res.ok) {
        console.error("Failed to discover schemes")
        return
      }

      const data = await res.json()
      setSchemes(data.schemes || [])
      if (data.schemes?.length > 0) {
        setSelectedScheme(data.schemes[0])
      }
    } catch (err) {
      console.error("Failed to discover schemes:", err)
    }
  }, [])

  // Fetch simulators and discover schemes on mount
  useEffect(() => {
    fetch("/api/simulator/list")
      .then((res) => res.json())
      .then((data) => {
        setSimulators(data.simulators || [])
        // Auto-select first booted simulator, or first available
        const booted = data.simulators?.find(
          (s: Simulator) => s.state === "Booted"
        )
        if (booted) {
          setSelectedSimulator(booted.udid)
        } else if (data.simulators?.length > 0) {
          setSelectedSimulator(data.simulators[0].udid)
        }
      })
      .catch((err) => console.error("Failed to fetch simulators:", err))

    // Discover schemes for default project path
    discoverSchemes(projectPath)
  }, [discoverSchemes, projectPath])

  // Subscribe to stream logs when streaming starts
  useEffect(() => {
    if (buildState.status !== "streaming") {
      return
    }

    setStreamLogs([]) // Clear previous logs

    const eventSource = new EventSource("/api/simulator/stream/logs")

    const formatLogEvent = (event: StreamLogEvent): string => {
      const timestamp = new Date().toLocaleTimeString()
      switch (event.type) {
        case "info":
          return `[${timestamp}] INFO: ${event.message}`
        case "error":
          return `[${timestamp}] ERROR: ${event.message}`
        case "debug":
          return `[${timestamp}] DEBUG: ${event.message}`
        case "frame":
          return `[${timestamp}] FRAME: #${event.frame_number}`
        default:
          return `[${timestamp}] ${JSON.stringify(event)}`
      }
    }

    eventSource.onmessage = (e) => {
      try {
        const event: StreamLogEvent = JSON.parse(e.data)
        setStreamLogs((prev) => [...prev.slice(-100), formatLogEvent(event)]) // Keep last 100 logs
      } catch {
        // Ignore parse errors
      }
    }

    eventSource.onerror = () => {
      setStreamLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] ERROR: Log stream disconnected`,
      ])
      eventSource.close()
    }

    return () => {
      eventSource.close()
    }
  }, [buildState.status])

  const handlePathChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const path = e.target.value
    setProjectPath(path)
  }

  const handlePathBlur = () => {
    if (projectPath) {
      discoverSchemes(projectPath)
    }
  }

  const handleBuildAndRun = async () => {
    if (!projectPath || !selectedScheme || !selectedSimulator) {
      setBuildState({
        status: "error",
        message: "Please select a project, scheme, and simulator",
      })
      return
    }

    setBuildState({ status: "building", lines: [] })

    try {
      // Stream the build output
      const res = await fetch("/api/xcode/build/stream", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          path: projectPath,
          scheme: selectedScheme,
        }),
      })

      if (!res.ok || !res.body) {
        throw new Error("Build request failed")
      }

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ""
      const lines: string[] = []
      let buildProducts: BuildProduct[] = []

      while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })

        // Process SSE events
        const eventEnd = buffer.lastIndexOf("\n\n")
        if (eventEnd === -1) continue

        const completeData = buffer.substring(0, eventEnd)
        buffer = buffer.substring(eventEnd + 2)

        for (const eventBlock of completeData.split("\n\n")) {
          const dataMatch = eventBlock.match(/^data: (.+)$/m)
          if (!dataMatch) continue

          try {
            const event = JSON.parse(dataMatch[1])

            if (event.type === "output") {
              lines.push(event.line)
              setBuildState({ status: "building", lines: [...lines] })
            } else if (event.type === "completed") {
              if (event.success) {
                buildProducts = event.products || []
              } else {
                throw new Error("Build failed")
              }
            } else if (event.type === "error") {
              throw new Error(event.message)
            }
          } catch {
            // Ignore parse errors for partial data
          }
        }
      }

      if (buildProducts.length === 0) {
        throw new Error("No build products found")
      }

      // Install and launch
      setBuildState({ status: "installing" })

      const launchRes = await fetch("/api/simulator/launch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          udid: selectedSimulator,
          app_path: buildProducts[0].path,
        }),
      })

      const launchData = await launchRes.json()

      if (!launchData.success) {
        throw new Error(launchData.message || "Failed to launch app")
      }

      // Start streaming at 60 FPS with good quality
      const url = `/api/simulator/stream?udid=${selectedSimulator}&fps=60&quality=0.7`
      setStreamUrl(url)
      setBuildState({ status: "streaming", udid: selectedSimulator })
    } catch (err) {
      setBuildState({
        status: "error",
        message: err instanceof Error ? err.message : "Unknown error",
      })
    }
  }

  const getStatusIcon = () => {
    switch (buildState.status) {
      case "building":
      case "installing":
        return <Loader2 className="w-4 h-4 animate-spin" />
      case "streaming":
      case "success":
        return <CheckCircle className="w-4 h-4 text-green-500" />
      case "error":
        return <XCircle className="w-4 h-4 text-red-500" />
      default:
        return <Play className="w-4 h-4" />
    }
  }

  const getStatusText = () => {
    switch (buildState.status) {
      case "building":
        return "Building..."
      case "installing":
        return "Installing..."
      case "streaming":
        return "Running"
      case "success":
        return "Build succeeded"
      case "error":
        return buildState.message
      default:
        return "Build & Run"
    }
  }

  const isLoading =
    buildState.status === "building" || buildState.status === "installing"

  return (
    <div className="h-screen w-screen flex flex-col bg-background text-foreground p-6 gap-6 overflow-hidden">
      {/* Header */}
      <header className="flex items-center gap-3 shrink-0">
        <img
          src="/plasma-icon.png"
          alt="Plasma"
          className="w-8 h-8 rounded"
        />
        <h1 className="text-xl font-semibold">Plasma</h1>
      </header>

      {/* Main content */}
      <div className="flex-1 flex gap-6 min-h-0 overflow-hidden">
        {/* Left side - Configuration */}
        <Card className="w-[400px] shrink-0 flex flex-col overflow-hidden">
          <CardHeader className="shrink-0">
            <CardTitle className="flex items-center gap-2">
              <FolderOpen className="w-5 h-5" />
              Project Configuration
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1 flex flex-col gap-4 overflow-y-auto">
            {/* Project Path */}
            <div className="flex flex-col gap-2">
              <label className="text-sm text-muted-foreground">
                Project Path
              </label>
              <Input
                type="text"
                placeholder="/path/to/your/project"
                value={projectPath}
                onChange={handlePathChange}
                onBlur={handlePathBlur}
              />
            </div>

            {/* Scheme Selector */}
            <div className="flex flex-col gap-2">
              <label className="text-sm text-muted-foreground">Scheme</label>
              <select
                className="w-full h-9 px-3 rounded-md border border-input bg-background text-sm"
                value={selectedScheme}
                onChange={(e) => setSelectedScheme(e.target.value)}
                disabled={schemes.length === 0}
              >
                {schemes.length === 0 ? (
                  <option value="">No schemes found</option>
                ) : (
                  schemes.map((scheme) => (
                    <option key={scheme} value={scheme}>
                      {scheme}
                    </option>
                  ))
                )}
              </select>
            </div>

            {/* Simulator Selector */}
            <div className="flex flex-col gap-2">
              <label className="text-sm text-muted-foreground">Simulator</label>
              <select
                className="w-full h-9 px-3 rounded-md border border-input bg-background text-sm"
                value={selectedSimulator}
                onChange={(e) => setSelectedSimulator(e.target.value)}
                disabled={simulators.length === 0}
              >
                {simulators.length === 0 ? (
                  <option value="">No simulators found</option>
                ) : (
                  simulators.map((sim) => (
                    <option key={sim.udid} value={sim.udid}>
                      {sim.name} {sim.state === "Booted" ? "(Booted)" : ""}
                    </option>
                  ))
                )}
              </select>
            </div>

            {/* Build & Run Button */}
            <Button
              className="w-full mt-2"
              onClick={handleBuildAndRun}
              disabled={isLoading || !projectPath || !selectedScheme}
            >
              {getStatusIcon()}
              <span className="ml-2">{getStatusText()}</span>
            </Button>

            {/* Build Output */}
            {buildState.status === "building" && buildState.lines.length > 0 && (
              <div className="flex flex-col gap-2">
                <label className="text-sm text-muted-foreground">
                  Build Output
                </label>
                <ScrollArea className="h-[200px] rounded-md border p-2 bg-black/20">
                  <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap">
                    {buildState.lines.slice(-50).join("\n")}
                  </pre>
                </ScrollArea>
              </div>
            )}

            {/* Stream Logs */}
            {buildState.status === "streaming" && (
              <div className="flex flex-col gap-2">
                <div
                  className="flex items-center justify-between cursor-pointer"
                  onClick={() => setShowLogs(!showLogs)}
                >
                  <label className="text-sm text-muted-foreground flex items-center gap-2">
                    <Terminal className="w-4 h-4" />
                    Stream Logs
                    {streamLogs.length > 0 && (
                      <span className="text-xs">({streamLogs.length})</span>
                    )}
                  </label>
                  {showLogs ? (
                    <ChevronUp className="w-4 h-4 text-muted-foreground" />
                  ) : (
                    <ChevronDown className="w-4 h-4 text-muted-foreground" />
                  )}
                </div>
                {showLogs && (
                  <ScrollArea className="h-[150px] rounded-md border p-2 bg-black/20">
                    <pre className="text-xs font-mono whitespace-pre-wrap">
                      {streamLogs.length > 0 ? (
                        streamLogs.map((log, i) => (
                          <div
                            key={i}
                            className={
                              log.includes("ERROR")
                                ? "text-red-400"
                                : log.includes("DEBUG")
                                ? "text-gray-500"
                                : log.includes("FRAME")
                                ? "text-green-400"
                                : "text-muted-foreground"
                            }
                          >
                            {log}
                          </div>
                        ))
                      ) : (
                        <span className="text-muted-foreground">
                          Waiting for logs...
                        </span>
                      )}
                    </pre>
                  </ScrollArea>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Right side - Simulator Stream (full height) */}
         <div className="flex-1 flex items-center justify-center min-w-0 min-h-0 overflow-hidden bg-black/20 rounded-xl">
           {streamUrl && buildState.status === "streaming" ? (
             <StreamViewer streamUrl={streamUrl} udid={(buildState as { udid: string }).udid} />
           ) : (
             <div className="flex flex-col items-center justify-center gap-4 text-muted-foreground">
               <div className="w-[200px] h-[400px] border-2 border-dashed border-border rounded-3xl flex items-center justify-center">
                 <span className="text-sm">Simulator</span>
               </div>
               <p className="text-sm">
                 Configure your project and click "Build & Run" to start
               </p>
             </div>
           )}
         </div>
      </div>
    </div>
  )
}
