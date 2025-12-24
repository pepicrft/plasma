import { useState, useEffect, useRef, useMemo } from "react"
import { useNavigate } from "react-router-dom"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { FolderOpen } from "lucide-react"

function getPlatformProjectTypes(): string {
  const platform = navigator.platform.toLowerCase()
  const isMacOS = platform.includes("mac")

  if (isMacOS) {
    return "Xcode or Android"
  }
  return "Android"
}

interface ValidateProjectResponse {
  valid: string
  type?: "xcode" | "android"
  name?: string
  /** Full path to the project file (.xcworkspace, .xcodeproj, or build.gradle) */
  path?: string
  error?: string
}

interface RecentProject {
  path: string
  name: string
  type: "xcode" | "android"
  valid: boolean
}

interface RecentProjectsResponse {
  projects: RecentProject[]
}

export function GetStarted() {
  const navigate = useNavigate()
  const [path, setPath] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [suggestions, setSuggestions] = useState<RecentProject[]>([])
  const [showSuggestions, setShowSuggestions] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(-1)
  const inputRef = useRef<HTMLInputElement>(null)
  const suggestionsRef = useRef<HTMLDivElement>(null)
  const projectTypes = useMemo(() => getPlatformProjectTypes(), [])

  // Fetch recent projects on mount and when typing
  useEffect(() => {
    const fetchProjects = async () => {
      try {
        const query = path ? `?query=${encodeURIComponent(path)}` : ""
        const response = await fetch(`/api/projects/recent${query}`)
        const data: RecentProjectsResponse = await response.json()
        // Only show valid projects
        setSuggestions(data.projects.filter((p) => p.valid))
      } catch {
        setSuggestions([])
      }
    }

    const debounce = setTimeout(fetchProjects, 150)
    return () => clearTimeout(debounce)
  }, [path])

  // Close suggestions when clicking outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (
        suggestionsRef.current &&
        !suggestionsRef.current.contains(e.target as Node) &&
        inputRef.current &&
        !inputRef.current.contains(e.target as Node)
      ) {
        setShowSuggestions(false)
      }
    }

    document.addEventListener("mousedown", handleClickOutside)
    return () => document.removeEventListener("mousedown", handleClickOutside)
  }, [])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setIsLoading(true)
    setShowSuggestions(false)

    try {
      const response = await fetch("/api/projects/validate", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ path }),
      })

      const data: ValidateProjectResponse = await response.json()

      if (data.valid === "true" && data.path) {
        // Use the resolved project file path, not the input directory
        navigate(`/?project=${encodeURIComponent(data.path)}`)
      } else if (data.valid === "false") {
        setError(data.error || "Invalid project directory")
      }
    } catch {
      setError("Failed to validate project. Please try again.")
    } finally {
      setIsLoading(false)
    }
  }

  const handleSelectProject = (project: RecentProject) => {
    setPath(project.path)
    setShowSuggestions(false)
    setSelectedIndex(-1)
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!showSuggestions || suggestions.length === 0) return

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault()
        setSelectedIndex((prev) =>
          prev < suggestions.length - 1 ? prev + 1 : prev
        )
        break
      case "ArrowUp":
        e.preventDefault()
        setSelectedIndex((prev) => (prev > 0 ? prev - 1 : -1))
        break
      case "Enter":
        if (selectedIndex >= 0) {
          e.preventDefault()
          handleSelectProject(suggestions[selectedIndex])
        }
        break
      case "Escape":
        setShowSuggestions(false)
        setSelectedIndex(-1)
        break
    }
  }

  return (
    <div className="h-screen w-screen flex items-center justify-center bg-background">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="flex justify-center mb-4">
            <img
              src="/plasma-icon.png"
              alt="Plasma"
              className="w-16 h-16"
            />
          </div>
          <CardTitle className="text-2xl">Welcome to Plasma</CardTitle>
          <CardDescription>
            Enter the path to your {projectTypes} project to get started
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2 relative">
              <Input
                ref={inputRef}
                type="text"
                placeholder="/path/to/your/project"
                value={path}
                onChange={(e) => {
                  setPath(e.target.value)
                  setShowSuggestions(true)
                  setSelectedIndex(-1)
                }}
                onFocus={() => setShowSuggestions(true)}
                onKeyDown={handleKeyDown}
                aria-label="Project path"
                disabled={isLoading}
                autoComplete="off"
              />
              {showSuggestions && suggestions.length > 0 && (
                <div
                  ref={suggestionsRef}
                  className="absolute z-10 w-full mt-1 bg-popover border border-border rounded-md shadow-lg max-h-60 overflow-auto"
                >
                  {suggestions.map((project, index) => (
                    <button
                      key={project.path}
                      type="button"
                      className={`w-full px-3 py-2 text-left hover:bg-accent transition-colors ${
                        index === selectedIndex ? "bg-accent" : ""
                      }`}
                      onClick={() => handleSelectProject(project)}
                    >
                      <div className="font-medium text-sm">{project.name}</div>
                      <div className="text-xs text-muted-foreground truncate">
                        {project.path}
                      </div>
                    </button>
                  ))}
                </div>
              )}
              {error && (
                <p className="text-sm text-destructive" role="alert">
                  {error}
                </p>
              )}
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={!path.trim() || isLoading}
            >
              <FolderOpen className="w-4 h-4 mr-2" />
              {isLoading ? "Validating..." : "Open Project"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
