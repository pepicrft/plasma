import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  FolderOpen,
  Play,
  Settings,
  MessageSquare,
  Code,
  Smartphone,
} from "lucide-react"

interface MainLayoutProps {
  projectPath: string
}

export function MainLayout({ projectPath }: MainLayoutProps) {
  return (
    <div className="h-screen w-screen flex flex-col bg-background text-foreground">
      {/* Header */}
      <header className="h-12 border-b border-border flex items-center px-4 justify-between shrink-0">
        <div className="flex items-center gap-2">
          <img
            src="/plasma-icon.png"
            alt="Plasma"
            className="w-6 h-6 rounded"
          />
          <span className="font-semibold">Plasma</span>
          <span className="text-sm text-muted-foreground ml-2">
            {projectPath.split("/").pop()}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm">
            <Settings className="w-4 h-4" />
          </Button>
        </div>
      </header>

      {/* Main content */}
      <ResizablePanelGroup direction="horizontal" className="flex-1">
        {/* Sidebar - File explorer */}
        <ResizablePanel defaultSize={20} minSize={15} maxSize={30}>
          <div className="h-full flex flex-col">
            <div className="p-3 border-b border-border">
              <Button
                variant="outline"
                size="sm"
                className="w-full justify-start gap-2"
              >
                <FolderOpen className="w-4 h-4" />
                Open Project
              </Button>
            </div>
            <ScrollArea className="flex-1 p-2">
              <div className="text-sm text-muted-foreground p-4 text-center">
                No project open
              </div>
            </ScrollArea>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* Center - Code editor + Chat */}
        <ResizablePanel defaultSize={50}>
          <ResizablePanelGroup direction="vertical">
            {/* Code editor area */}
            <ResizablePanel defaultSize={60}>
              <div className="h-full flex flex-col">
                <div className="h-10 border-b border-border flex items-center px-3 gap-2">
                  <Code className="w-4 h-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Editor</span>
                </div>
                <ScrollArea className="flex-1 p-4">
                  <div className="text-sm text-muted-foreground text-center py-8">
                    Select a file to edit
                  </div>
                </ScrollArea>
              </div>
            </ResizablePanel>

            <ResizableHandle withHandle />

            {/* Chat area */}
            <ResizablePanel defaultSize={40} minSize={20}>
              <div className="h-full flex flex-col">
                <div className="h-10 border-b border-border flex items-center px-3 gap-2">
                  <MessageSquare className="w-4 h-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">
                    AI Assistant
                  </span>
                </div>
                <ScrollArea className="flex-1 p-4">
                  <div className="text-sm text-muted-foreground text-center py-4">
                    Describe what you want to build...
                  </div>
                </ScrollArea>
                <Separator />
                <div className="p-3">
                  <div className="flex gap-2">
                    <Textarea
                      placeholder="Ask the AI to help you build your app..."
                      className="min-h-[60px] resize-none"
                    />
                    <Button size="sm" className="self-end">
                      Send
                    </Button>
                  </div>
                </div>
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* Right - Preview/Simulator */}
        <ResizablePanel defaultSize={30} minSize={20}>
          <div className="h-full flex flex-col">
            <div className="h-10 border-b border-border flex items-center px-3 justify-between">
              <div className="flex items-center gap-2">
                <Smartphone className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm text-muted-foreground">Preview</span>
              </div>
              <Button variant="ghost" size="sm">
                <Play className="w-4 h-4" />
              </Button>
            </div>
            <div className="flex-1 flex items-center justify-center bg-black/5">
              <div className="w-[280px] h-[560px] bg-background border border-border rounded-3xl shadow-lg flex items-center justify-center">
                <span className="text-sm text-muted-foreground">Simulator</span>
              </div>
            </div>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}
