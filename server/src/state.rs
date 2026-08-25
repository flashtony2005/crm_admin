//! 共享状态
use crate::cmsdb::CmsDb;

#[derive(Clone)]
pub struct AppState {
    pub db: CmsDb,
    /// Phase 1 单租户；Phase 4 数据权限时从请求上下文解析
    pub tenant: String,
}
