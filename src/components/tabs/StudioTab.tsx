/**
 * StudioTab — 创作 (media generation studio).
 *
 * Image generation (sync) + async video tasks against any enabled
 * OpenAI-compatible media provider (Agnes AI first — see BORROWINGS.md #9).
 * Artifacts live in ~/.omnix/media; this panel is the unified gallery.
 * Video progress arrives via the backend poller's `media-task-update` events.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Clapperboard, Image as ImageIcon, Loader2, RefreshCw, Trash2, Wand2 } from "lucide-react";
import { toast } from "sonner";

import { mediaApi, platformApi } from "@/lib/tauri-api";
import type { MediaModelSuggestions, MediaTask, ModelPlatform } from "@/types";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

const IMAGE_SIZES = ["1024x1024", "1024x768", "768x1024", "1536x640", "640x1536"];
const VIDEO_SIZES = [
  { label: "720p 横屏 16:9", width: 1280, height: 720 },
  { label: "720p 竖屏 9:16", width: 720, height: 1280 },
  { label: "1:1 方形", width: 960, height: 960 },
  { label: "1152×768", width: 1152, height: 768 },
];
const VIDEO_SECONDS = [3, 5, 8, 10, 15];
const VIDEO_FPS = 24;

/** Gallery cell: images load as data URLs; videos stream via asset://. */
function MediaThumb({
  task,
  onDelete,
  onToVideo,
}: {
  task: MediaTask;
  onDelete: (id: string) => void;
  onToVideo?: (task: MediaTask) => void;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const isVideo = task.kind === "video";

  useEffect(() => {
    let cancelled = false;
    if (task.status !== "completed" || !task.result_path) {
      setSrc(null);
      return;
    }
    if (isVideo) {
      setSrc(convertFileSrc(task.result_path));
    } else {
      mediaApi.readFile(task.id)
        .then((dataUrl) => { if (!cancelled) setSrc(dataUrl); })
        .catch(() => { if (!cancelled) setFailed(true); });
    }
    return () => { cancelled = true; };
  }, [task.id, task.status, task.result_path, isVideo]);

  return (
    <div className="group relative overflow-hidden rounded-lg border border-border bg-card/40">
      <div className={cn("flex items-center justify-center bg-muted/20", isVideo ? "aspect-video" : "aspect-square")}>
        {task.status === "failed" ? (
          <div className="p-3 text-center text-xs text-destructive" title={task.error ?? ""}>
            生成失败
            <div className="mt-1 line-clamp-3 text-[10px] text-muted-foreground">{task.error}</div>
          </div>
        ) : task.status !== "completed" ? (
          <div className="flex flex-col items-center gap-2 p-3 text-xs text-muted-foreground">
            <Loader2 className="h-5 w-5 animate-spin" />
            {isVideo ? (
              <>
                <div>{task.status === "pending" ? "排队中…" : `生成中 ${task.progress}%`}</div>
                <div className="h-1 w-24 overflow-hidden rounded-full bg-muted/40">
                  <div className="h-full bg-primary transition-all" style={{ width: `${task.progress}%` }} />
                </div>
              </>
            ) : (
              <div>生成中…</div>
            )}
          </div>
        ) : isVideo && src ? (
          <video src={src} controls className="h-full w-full object-contain" />
        ) : src ? (
          <img src={src} alt={task.prompt} className="h-full w-full object-cover" />
        ) : failed ? (
          <span className="text-xs text-muted-foreground">无法读取文件</span>
        ) : (
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        )}
      </div>
      <div className="p-2">
        <div className="line-clamp-2 text-xs text-muted-foreground" title={task.prompt}>
          {task.prompt}
        </div>
        <div className="mt-1 flex items-center justify-between gap-1">
          <Badge variant="secondary" className="truncate text-[10px]">{task.model}</Badge>
          <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
            {!isVideo && task.status === "completed" && onToVideo && (
              <button
                className="rounded p-1 text-muted-foreground hover:text-primary"
                title="以这张图生成视频"
                onClick={() => onToVideo(task)}
              >
                <Clapperboard className="h-3.5 w-3.5" />
              </button>
            )}
            <button
              className="rounded p-1 text-muted-foreground hover:text-destructive"
              title="删除（同时删除文件）"
              onClick={() => onDelete(task.id)}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function StudioTab() {
  const [platforms, setPlatforms] = useState<ModelPlatform[]>([]);
  const [suggestions, setSuggestions] = useState<MediaModelSuggestions>({ image: [], video: [] });
  const [tasks, setTasks] = useState<MediaTask[]>([]);

  const [mode, setMode] = useState<"image" | "video">("image");
  const [platformId, setPlatformId] = useState("");
  const [imageModel, setImageModel] = useState("");
  const [videoModel, setVideoModel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [size, setSize] = useState(IMAGE_SIZES[0]);
  const [videoSizeIdx, setVideoSizeIdx] = useState(0);
  const [seconds, setSeconds] = useState(5);
  const [sourceImage, setSourceImage] = useState<MediaTask | null>(null);
  const [generating, setGenerating] = useState(false);

  const loadTasks = useCallback(async () => {
    try {
      setTasks(await mediaApi.listTasks());
    } catch (error) {
      toast.error(`读取创作记录失败：${error}`);
    }
  }, []);

  useEffect(() => {
    platformApi.list()
      .then((list) => {
        const enabled = list.filter((platform) => platform.is_enabled);
        setPlatforms(enabled);
        const agnes = enabled.find((platform) =>
          platform.api_address.includes("agnes-ai") || platform.id.includes("agnes"));
        setPlatformId((current) => current || agnes?.id || enabled[0]?.id || "");
      })
      .catch(() => setPlatforms([]));
    mediaApi.modelSuggestions()
      .then((value) => {
        setSuggestions(value);
        setImageModel((current) => current || value.image[0] || "");
        setVideoModel((current) => current || value.video[0] || "");
      })
      .catch(() => undefined);
    void loadTasks();
  }, [loadTasks]);

  // Live progress from the backend video poller.
  useEffect(() => {
    const unlisten = listen<MediaTask>("media-task-update", (event) => {
      const updated = event.payload;
      setTasks((current) => {
        const index = current.findIndex((task) => task.id === updated.id);
        if (index === -1) return [updated, ...current];
        const next = [...current];
        next[index] = updated;
        return next;
      });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const generate = useCallback(async () => {
    if (!platformId) {
      toast.error("请先在「模型中心」添加并启用一个媒体供应商（如 Agnes AI）");
      return;
    }
    if (!prompt.trim()) {
      toast.error("请填写提示词");
      return;
    }
    setGenerating(true);
    try {
      if (mode === "image") {
        if (!imageModel.trim()) { toast.error("请填写生图模型"); return; }
        await mediaApi.generateImage(platformId, imageModel.trim(), prompt.trim(), size);
        toast.success("图片已生成");
        setPrompt("");
      } else {
        if (!videoModel.trim()) { toast.error("请填写视频模型"); return; }
        const preset = VIDEO_SIZES[videoSizeIdx];
        // Backend snaps num_frames onto the provider's 8n+1 grid.
        await mediaApi.createVideoTask(
          platformId,
          videoModel.trim(),
          prompt.trim(),
          preset.width,
          preset.height,
          seconds * VIDEO_FPS + 1,
          VIDEO_FPS,
          sourceImage?.id ?? null,
        );
        toast.success("视频任务已提交，进度见画廊");
        setPrompt("");
        setSourceImage(null);
      }
    } catch (error) {
      toast.error(`生成失败：${error}`);
    } finally {
      setGenerating(false);
      void loadTasks();
    }
  }, [platformId, mode, imageModel, videoModel, prompt, size, videoSizeIdx, seconds, sourceImage, loadTasks]);

  const deleteTask = useCallback(async (taskId: string) => {
    try {
      await mediaApi.deleteTask(taskId);
      setTasks((current) => current.filter((task) => task.id !== taskId));
    } catch (error) {
      toast.error(`删除失败：${error}`);
    }
  }, []);

  const toVideo = useCallback((task: MediaTask) => {
    setMode("video");
    setSourceImage(task);
    setPrompt(task.prompt);
    toast.message("已切到「生视频」，将以所选图片作为首帧");
  }, []);

  const videoTasks = useMemo(() => tasks.filter((task) => task.kind === "video"), [tasks]);
  const imageTasks = useMemo(() => tasks.filter((task) => task.kind === "image"), [tasks]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl space-y-6">
        {/* Composer */}
        <div className="rounded-lg border border-border bg-card/40 p-5">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-base font-semibold">
              <Wand2 className="h-4 w-4 text-primary" /> 创作 Studio
            </div>
            <div className="flex overflow-hidden rounded-md border border-border text-sm">
              <button
                className={cn("px-3 py-1.5", mode === "image" ? "bg-primary/15 text-primary" : "text-muted-foreground hover:text-foreground")}
                onClick={() => setMode("image")}
              >
                文生图
              </button>
              <button
                className={cn("px-3 py-1.5", mode === "video" ? "bg-primary/15 text-primary" : "text-muted-foreground hover:text-foreground")}
                onClick={() => setMode("video")}
              >
                生视频
              </button>
            </div>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            使用「模型中心」里已启用的媒体供应商（首个支持：Agnes AI）。
          </p>

          {mode === "video" && sourceImage && (
            <div className="mt-3 flex items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-xs">
              <Clapperboard className="h-3.5 w-3.5 text-primary" />
              图生视频：以「{sourceImage.prompt.slice(0, 30)}…」的图片为首帧
              <button className="ml-auto text-muted-foreground hover:text-foreground" onClick={() => setSourceImage(null)}>
                取消
              </button>
            </div>
          )}

          <Textarea
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            placeholder={mode === "image"
              ? "描述你想生成的画面，例如：黎明时分漂浮在峡谷云海之上的发光城市，电影质感"
              : "描述视频画面与运动，例如：一只猫在日落的海滩上缓慢走过，镜头跟随"}
            className="mt-3 min-h-20"
          />

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <select
              value={platformId}
              onChange={(event) => setPlatformId(event.target.value)}
              className="h-8 max-w-48 rounded-md border border-border bg-background px-2 text-sm"
              title="媒体供应商（模型中心里已启用的平台）"
            >
              {platforms.length === 0 && <option value="">（无可用平台）</option>}
              {platforms.map((platform) => (
                <option key={platform.id} value={platform.id}>{platform.name}</option>
              ))}
            </select>

            {mode === "image" ? (
              <>
                <input
                  value={imageModel}
                  onChange={(event) => setImageModel(event.target.value)}
                  list="studio-image-models"
                  placeholder="模型，如 agnes-image-2.1-flash"
                  className="h-8 w-56 rounded-md border border-border bg-background px-2 text-sm"
                />
                <datalist id="studio-image-models">
                  {suggestions.image.map((name) => <option key={name} value={name} />)}
                </datalist>
                <select
                  value={size}
                  onChange={(event) => setSize(event.target.value)}
                  className="h-8 rounded-md border border-border bg-background px-2 text-sm"
                  title="输出尺寸"
                >
                  {IMAGE_SIZES.map((option) => <option key={option} value={option}>{option}</option>)}
                </select>
              </>
            ) : (
              <>
                <input
                  value={videoModel}
                  onChange={(event) => setVideoModel(event.target.value)}
                  list="studio-video-models"
                  placeholder="模型，如 agnes-video-v2.0"
                  className="h-8 w-52 rounded-md border border-border bg-background px-2 text-sm"
                />
                <datalist id="studio-video-models">
                  {suggestions.video.map((name) => <option key={name} value={name} />)}
                </datalist>
                <select
                  value={videoSizeIdx}
                  onChange={(event) => setVideoSizeIdx(Number(event.target.value))}
                  className="h-8 rounded-md border border-border bg-background px-2 text-sm"
                  title="分辨率"
                >
                  {VIDEO_SIZES.map((option, index) => (
                    <option key={option.label} value={index}>{option.label}</option>
                  ))}
                </select>
                <select
                  value={seconds}
                  onChange={(event) => setSeconds(Number(event.target.value))}
                  className="h-8 rounded-md border border-border bg-background px-2 text-sm"
                  title="时长（秒），24fps"
                >
                  {VIDEO_SECONDS.map((option) => <option key={option} value={option}>{option} 秒</option>)}
                </select>
              </>
            )}

            <div className="flex-1" />
            <Button onClick={() => void generate()} disabled={generating}>
              {generating
                ? <Loader2 className="h-4 w-4 animate-spin" />
                : mode === "image" ? <ImageIcon className="h-4 w-4" /> : <Clapperboard className="h-4 w-4" />}
              {generating ? "提交中…" : mode === "image" ? "生成图片" : "生成视频"}
            </Button>
          </div>
        </div>

        {/* Gallery */}
        <div className="rounded-lg border border-border bg-card/40 p-5">
          <div className="mb-3 flex items-center justify-between">
            <div className="text-sm font-medium">作品画廊（{tasks.length}）</div>
            <Button size="sm" variant="outline" onClick={() => void loadTasks()} title="刷新">
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          </div>
          {tasks.length === 0 ? (
            <div className="py-10 text-center text-sm text-muted-foreground">
              还没有作品。写一段提示词，生成你的第一张图或第一支视频。
            </div>
          ) : (
            <div className="space-y-4">
              {videoTasks.length > 0 && (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  {videoTasks.map((task) => (
                    <MediaThumb key={task.id} task={task} onDelete={(id) => void deleteTask(id)} />
                  ))}
                </div>
              )}
              {imageTasks.length > 0 && (
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                  {imageTasks.map((task) => (
                    <MediaThumb key={task.id} task={task} onDelete={(id) => void deleteTask(id)} onToVideo={toVideo} />
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
