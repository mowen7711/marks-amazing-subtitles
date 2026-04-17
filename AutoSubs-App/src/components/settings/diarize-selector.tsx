import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Slider } from "@/components/ui/slider"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useSettings } from "@/contexts/SettingsContext"
import { useTranslation } from "react-i18next"
import { open } from "@tauri-apps/plugin-dialog"
import { downloadDir } from "@tauri-apps/api/path"
import { X, Plus, Mic } from "lucide-react"
import { VoiceSample } from "@/types/interfaces"

const SUPPORTED_AUDIO_EXTENSIONS = [
    "wav", "mp3", "m4a", "aac", "flac", "ogg", "opus",
    "mp4", "mov", "mkv", "avi", "webm",
]

export function SpeakerSelector() {
    const { t } = useTranslation()
    const { settings, updateSetting } = useSettings()

    const handleAddSample = async () => {
        const file = await open({
            multiple: false,
            directory: false,
            filters: [{
                name: "Audio / Video",
                extensions: SUPPORTED_AUDIO_EXTENSIONS,
            }],
            defaultPath: await downloadDir(),
        })
        if (!file) return

        const label = `Speaker ${settings.voiceSamples.length + 1}`
        const newSamples: VoiceSample[] = [
            ...settings.voiceSamples,
            { label, path: file as string },
        ]
        updateSetting("voiceSamples", newSamples)
    }

    const handleRemoveSample = (index: number) => {
        const newSamples = settings.voiceSamples.filter((_, i) => i !== index)
        updateSetting("voiceSamples", newSamples)
    }

    const handleRenameLabel = (index: number, value: string) => {
        const newSamples = settings.voiceSamples.map((s, i) =>
            i === index ? { ...s, label: value } : s
        )
        updateSetting("voiceSamples", newSamples)
    }

    const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path

    return (
        <>
            <div className="px-4 py-5">
                {/* Speaker Count Slider */}
                <div className="space-y-3">
                    <div className="flex items-center justify-between">
                        <Label className="text-sm font-medium">{t("actionBar.speakers.countTitle")}</Label>
                        <span className={`text-sm font-medium ${settings.enableDiarize ? "text-primary" : "text-red-500"}`}>
                            {settings.enableDiarize
                                ? (settings.maxSpeakers === null
                                    ? t("actionBar.common.auto")
                                    : settings.maxSpeakers)
                                : t("actionBar.speakers.disabled")}
                        </span>
                    </div>
                    <Slider
                        value={[settings.enableDiarize ? (settings.maxSpeakers === null ? 0 : settings.maxSpeakers) : 0]}
                        onValueChange={([value]: [number]) =>
                            settings.enableDiarize && updateSetting("maxSpeakers", value === 0 ? null : value)
                        }
                        max={10}
                        min={0}
                        step={1}
                        className="w-full"
                        disabled={!settings.enableDiarize}
                    />
                    <div className="flex justify-between text-xs text-muted-foreground">
                        <span>{t("actionBar.common.auto")}</span>
                        <span>10</span>
                    </div>
                </div>
            </div>

            {/* Voice Filter Section */}
            {settings.enableDiarize && (
                <div className="border-t">
                    <div className="px-4 py-4 space-y-4">
                        {/* Toggle */}
                        <div className="flex items-center justify-between">
                            <div className="space-y-0.5">
                                <Label className="text-sm font-medium flex items-center gap-1.5">
                                    <Mic className="w-3.5 h-3.5" />
                                    Voice Filter
                                    {settings.voiceFilterEnabled && settings.voiceSamples.length > 0 && (
                                        <span className="inline-flex items-center justify-center h-4 min-w-4 px-1 rounded-full bg-primary text-primary-foreground text-[10px] font-semibold">
                                            {settings.voiceSamples.length}
                                        </span>
                                    )}
                                </Label>
                                <p className="text-xs text-muted-foreground">
                                    Only transcribe segments matching sample voices
                                </p>
                            </div>
                            <Switch
                                checked={settings.voiceFilterEnabled}
                                onCheckedChange={(checked: boolean) =>
                                    updateSetting("voiceFilterEnabled", checked)
                                }
                            />
                        </div>

                        {settings.voiceFilterEnabled && (
                            <div className="space-y-3">
                                {/* Sample list */}
                                {settings.voiceSamples.length === 0 && (
                                    <p className="text-xs text-muted-foreground text-center py-1">
                                        Add a short audio clip of your voice to filter out background speakers
                                    </p>
                                )}
                                {settings.voiceSamples.length > 0 && (
                                    <div className="space-y-2">
                                        {settings.voiceSamples.map((sample, index) => (
                                            <div key={index} className="flex items-center gap-2">
                                                <Input
                                                    value={sample.label}
                                                    onChange={(e) => handleRenameLabel(index, e.target.value)}
                                                    className="h-7 text-xs w-28 shrink-0"
                                                    placeholder="Label"
                                                />
                                                <span
                                                    className="text-xs text-muted-foreground truncate flex-1 min-w-0"
                                                    title={sample.path}
                                                >
                                                    {fileName(sample.path)}
                                                </span>
                                                <Button
                                                    variant="ghost"
                                                    size="icon"
                                                    className="h-6 w-6 shrink-0 text-muted-foreground hover:text-destructive"
                                                    onClick={() => handleRemoveSample(index)}
                                                >
                                                    <X className="h-3 w-3" />
                                                </Button>
                                            </div>
                                        ))}
                                    </div>
                                )}

                                {/* Add sample button */}
                                <Button
                                    variant="outline"
                                    size="sm"
                                    className="w-full h-8 text-xs gap-1.5"
                                    onClick={handleAddSample}
                                >
                                    <Plus className="h-3 w-3" />
                                    Add Voice Sample
                                </Button>

                                {/* Similarity threshold */}
                                <div className="space-y-2">
                                    <div className="flex items-center justify-between">
                                        <Label className="text-xs text-muted-foreground">Match Sensitivity</Label>
                                        <span className="text-xs font-medium">
                                            {Math.round(settings.voiceSimilarityThreshold * 100)}%
                                        </span>
                                    </div>
                                    <Slider
                                        value={[settings.voiceSimilarityThreshold]}
                                        onValueChange={([value]: [number]) =>
                                            updateSetting("voiceSimilarityThreshold", value)
                                        }
                                        min={0.5}
                                        max={0.95}
                                        step={0.05}
                                        className="w-full"
                                    />
                                    <div className="flex justify-between text-xs text-muted-foreground">
                                        <span>Loose</span>
                                        <span>Strict</span>
                                    </div>
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            )}

            <div className="border-t bg-muted/30">
                <div className="p-4 pt-2 space-y-2">
                    <div className="flex items-center justify-between">
                        <div className="space-y-0.5">
                            <Label className="text-sm font-medium">{t("actionBar.speakers.title")}</Label>
                            <p className="text-xs text-muted-foreground">{t("actionBar.speakers.description")}</p>
                        </div>
                        <Switch
                            checked={settings.enableDiarize}
                            onCheckedChange={(checked: boolean) => updateSetting("enableDiarize", checked)}
                        />
                    </div>
                </div>
            </div>
        </>
    )
}
