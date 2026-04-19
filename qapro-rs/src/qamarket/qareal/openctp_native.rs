#![allow(dead_code)]
//! openctp 自研接入层骨架。
//!
//! 当前状态：
//! - 已暂停，不作为第一阶段主路线
//! - 第一阶段主路线已切回 `ctp2rs`
//! - 本文件仅保留为备用设计占位

pub mod ffi {
    // 预留：后续在这里放置最小 CTP/OpenCTP FFI 声明。
    // 第一阶段先冻结模块边界，避免上层直接依赖裸 FFI。
}

pub mod loader {
    /// 已加载动态库句柄占位。
    #[derive(Debug, Clone)]
    pub struct DynamicLibraryHandle {
        pub path: String,
    }

    impl DynamicLibraryHandle {
        pub fn new(path: &str) -> Result<Self, String> {
            if path.is_empty() {
                return Err("dynamic library path 不能为空".to_string());
            }
            Ok(Self {
                path: path.to_string(),
            })
        }
    }
}

pub mod md {
    use super::loader::DynamicLibraryHandle;

    /// openctp 行情会话占位。
    #[derive(Debug, Clone)]
    pub struct MdSession {
        pub handle: DynamicLibraryHandle,
        pub front: String,
        pub user_id: String,
    }

    impl MdSession {
        pub fn new(handle: DynamicLibraryHandle, front: &str, user_id: &str) -> Self {
            Self {
                handle,
                front: front.to_string(),
                user_id: user_id.to_string(),
            }
        }
    }
}

pub mod td {
    use super::loader::DynamicLibraryHandle;

    /// openctp 交易会话占位。
    #[derive(Debug, Clone)]
    pub struct TdSession {
        pub handle: DynamicLibraryHandle,
        pub front: String,
        pub broker_id: String,
        pub user_id: String,
    }

    impl TdSession {
        pub fn new(
            handle: DynamicLibraryHandle,
            front: &str,
            broker_id: &str,
            user_id: &str,
        ) -> Self {
            Self {
                handle,
                front: front.to_string(),
                broker_id: broker_id.to_string(),
                user_id: user_id.to_string(),
            }
        }
    }
}
