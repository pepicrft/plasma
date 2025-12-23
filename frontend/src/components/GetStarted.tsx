import { useState } from "react"
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

interface ValidateProjectResponse {
  valid: string
  type?: "xcode" | "android"
  name?: string
  error?: string
}

export function GetStarted() {
  const navigate = useNavigate()
  const [path, setPath] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setIsLoading(true)

    try {
      const response = await fetch("/api/projects/validate", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ path }),
      })

      const data: ValidateProjectResponse = await response.json()

      if (data.valid === "true") {
        navigate(`/?project=${encodeURIComponent(path)}`)
      } else {
        setError(data.error || "Invalid project directory")
      }
    } catch {
      setError("Failed to validate project. Please try again.")
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <div className="h-screen w-screen flex items-center justify-center bg-background">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="flex justify-center mb-4">
            <img
              src="/appwave-icon.png"
              alt="Appwave"
              className="w-16 h-16"
            />
          </div>
          <CardTitle className="text-2xl">Welcome to Appwave</CardTitle>
          <CardDescription>
            Enter the path to your Xcode or Android project to get started
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Input
                type="text"
                placeholder="/path/to/your/project"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                aria-label="Project path"
                disabled={isLoading}
              />
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
