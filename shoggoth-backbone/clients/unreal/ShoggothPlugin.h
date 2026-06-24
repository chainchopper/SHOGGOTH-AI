// Shoggoth Unreal Engine Plugin — Module Interface
//
// Provides Blueprint-callable functions and C++ classes for integrating
// Unreal Engine 5 projects with the Shoggoth Mesh Machine.
//
// Installation:
//   Copy this folder to YourProject/Plugins/Shoggoth/
//   Add "Shoggoth" to PublicDependencyModuleNames in YourProject.Build.cs
//
// Features:
//   • Auto-detects Shoggoth orchestrator on the local network.
//   • One-click render farm distribution: splits viewport tiles across fabric GPUs.
//   • Nanite/Lumen shard routing for real-time path tracing acceleration.
//   • Blueprint nodes: GetTopology, AnalyzeScene, LaunchRenderFarm.
//   • Movie Render Queue integration: distributed frame rendering.

#pragma once

#include "CoreMinimal.h"
#include "Modules/ModuleManager.h"
#include "Http.h"
#include "Json.h"
#include "WebSocketsModule.h"
#include "IWebSocket.h"

class FShoggothModule : public IModuleInterface
{
public:
    virtual void StartupModule() override;
    virtual void ShutdownModule() override;

    /** Returns the orchestrator base URL (auto-discovered or from project settings). */
    static FString GetOrchestratorUrl();

    /** Sends an HTTP GET to the orchestrator and returns the response body. */
    static void HttpGet(const FString& Path, TFunction<void(bool bSuccess, const FString& Response)> Callback);

    /** Sends an HTTP POST with JSON body. */
    static void HttpPost(const FString& Path, const FString& JsonBody, TFunction<void(bool bSuccess, const FString& Response)> Callback);

private:
    static FString OrchestratorUrl;
    TSharedPtr<IWebSocket> TelemetrySocket;
};

// ── Data Structures ───────────────────────────────────────────────────────────

USTRUCT(BlueprintType)
struct FShoggothNodeInfo
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly) FString NodeId;
    UPROPERTY(BlueprintReadOnly) FString Tier;
    UPROPERTY(BlueprintReadOnly) int32 VramGb = 0;
    UPROPERTY(BlueprintReadOnly) float PingMs = 0.f;
    UPROPERTY(BlueprintReadOnly) bool bAcceptingWork = false;
    UPROPERTY(BlueprintReadOnly) float TemperatureC = 0.f;
};

USTRUCT(BlueprintType)
struct FShoggothTopology
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly) int32 TotalNodes = 0;
    UPROPERTY(BlueprintReadOnly) float TotalVramGb = 0.f;
    UPROPERTY(BlueprintReadOnly) int32 FullShoggoths = 0;
    UPROPERTY(BlueprintReadOnly) TArray<FShoggothNodeInfo> Nodes;
    UPROPERTY(BlueprintReadOnly) int32 UptimeSeconds = 0;
};

USTRUCT(BlueprintType)
struct FShoggothAnalysisResult
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly) FString Workload;
    UPROPERTY(BlueprintReadOnly) FString TargetNode;
    UPROPERTY(BlueprintReadOnly) FString Reason;
    UPROPERTY(BlueprintReadOnly) FString SuggestedTemplate;
    UPROPERTY(BlueprintReadOnly) FString TemplateManifest;
    UPROPERTY(BlueprintReadOnly) float Confidence = 0.f;
};

// ── Blueprint Function Library ────────────────────────────────────────────────

UCLASS()
class SHOGGOTH_API UShoggothBlueprintLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:
    /** Fetches the current hardware fabric topology. */
    UFUNCTION(BlueprintCallable, Category = "Shoggoth|Topology")
    static void GetTopology(const FString& OrchestratorUrl, TFunction<void(bool bSuccess, FShoggothTopology Topology)> Callback);

    /** Analyzes source code or shader and returns optimal hardware routing. */
    UFUNCTION(BlueprintCallable, Category = "Shoggoth|Analysis")
    static void AnalyzeWorkload(const FString& SourceCode, TFunction<void(bool bSuccess, FShoggothAnalysisResult Result)> Callback);

    /** Launches a render farm template for distributed Unreal rendering. */
    UFUNCTION(BlueprintCallable, Category = "Shoggoth|Launch")
    static void LaunchRenderFarm(const FString& ProjectName, TFunction<void(bool bSuccess, FString Message)> Callback);

    /** Connects to the live telemetry WebSocket feed. */
    UFUNCTION(BlueprintCallable, Category = "Shoggoth|Telemetry")
    static void ConnectTelemetry(const FString& OrchestratorUrl);

    /** Disconnects from telemetry. */
    UFUNCTION(BlueprintCallable, Category = "Shoggoth|Telemetry")
    static void DisconnectTelemetry();
};

// ── Movie Render Queue Integration ────────────────────────────────────────────

/**
 * Custom Movie Render Pipeline that distributes frames across the Shoggoth fabric.
 *
 * Usage:
 *   1. Add a "Shoggoth Render Farm" output to your Movie Render Queue.
 *   2. Set the orchestrator URL.
 *   3. Render — frames are automatically distributed to available GPUs.
 */
UCLASS()
class SHOGGOTH_API UShoggothMoviePipeline : public UObject
{
    GENERATED_BODY()

public:
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Shoggoth")
    FString OrchestratorUrl = TEXT("http://localhost:9100");

    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Shoggoth")
    int32 TilesPerFrame = 4;  // Split each frame into N tiles for GPU distribution.

    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Shoggoth")
    bool bUseCloudNodes = true;  // Allow cloud GPUs if local pool saturated.
};
