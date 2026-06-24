// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-display/src/lib.rs — Display compositor and streaming engine.
//
// Provides the sub-millisecond video compositing pipeline that merges frame
// buffers from disparate GPUs (5090, 4090, BC250 grid, V620, cloud nodes)
// into a single cohesive viewport stream delivered via WebRTC with adaptive
// bitrate and hardware-accelerated encoding.

pub mod client_stream;
pub mod compositor;
pub mod hardware_encoder;
pub mod network_shading;
pub mod webrtc_signaling;

pub use compositor::{RenderFrameFragment, ShoggothCompositor};
pub use hardware_encoder::{detect_best_encoder, EncoderBackend, EncodedFrame, ShoggothStreamingEncoder, VideoCodec};
pub use network_shading::SpatialViewportDelta;
pub use client_stream::ClientStreamController;
