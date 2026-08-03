use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

pub type SharedDraft<T> = Arc<T>;
// (committed_snapshot, optional_draft_snapshot)
type DraftData<T> = (SharedDraft<T>, Option<SharedDraft<T>>);

/// 草稿保存的是「已提交数据 + 尚未生效的改动」的整份快照。
///
/// 如果在草稿存活期间，已提交数据被 [`Draft::with_data_modify`] 改写（例如订阅
/// 刷新写入了新的 items），草稿就落后于已提交数据了；此时 [`Draft::apply`] 会用
/// 过时的整份快照覆盖已提交数据，把中间那次更新静默回滚掉——表现为「界面上的数据
/// 要重启应用才更新」，而磁盘上其实已经是新的。
///
/// 实现本 trait 来声明「草稿到底只承载哪些改动」，`with_data_modify` 会在提交新
/// 数据之后把这些改动重新落到新数据上，让草稿始终基于最新的已提交数据。
pub trait DraftRebase {
    /// 把 `self`（已过时的草稿）真正想表达的改动，重新应用到 `newer`
    /// （刚提交的最新数据）上。
    fn rebase_onto(&self, newer: &mut Self);
}

#[derive(Debug)]
struct DraftInner<T> {
    data: RwLock<DraftData<T>>,
    /// 串行化 [`Draft::with_data_modify`]：并发调用排队执行，而不是让后来者
    /// 看到过期快照后被丢弃（旧实现用乐观锁检测冲突，冲突时改动直接丢失）。
    data_modify_lock: AsyncMutex<()>,
}

/// Draft 管理：committed 与 optional draft 都以 Arc<T> 存储
#[derive(Debug)]
pub struct Draft<T> {
    inner: Arc<DraftInner<T>>,
}

impl<T: Clone> Draft<T> {
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(DraftInner {
                data: RwLock::new((Arc::new(data), None)),
                data_modify_lock: AsyncMutex::new(()),
            }),
        }
    }

    /// 以 Arc<T> 的形式获取当前“已提交（正式）”数据的快照（零拷贝，仅 clone Arc）
    #[inline]
    pub fn data_arc(&self) -> SharedDraft<T> {
        let guard = self.inner.data.read();
        Arc::clone(&guard.0)
    }

    /// 获取当前（草稿若存在则返回草稿，否则返回已提交）的快照
    /// 这也是零拷贝：只 clone Arc，不 clone T
    #[inline]
    pub fn latest_arc(&self) -> SharedDraft<T> {
        let guard = self.inner.data.read();
        guard.1.clone().unwrap_or_else(|| Arc::clone(&guard.0))
    }

    /// 通过闭包以可变方式编辑草稿（在闭包中我们给出 &mut T）
    /// - 延迟拷贝：如果只有这一个 Arc 引用，则直接修改，不会克隆 T；
    /// - 若草稿被其他读者共享，Arc::make_mut 会做一次 T.clone（最小必要拷贝）。
    #[inline]
    pub fn edit_draft<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut guard = self.inner.data.write();
        let mut draft_arc = guard.1.take().unwrap_or_else(|| Arc::clone(&guard.0));
        let data_mut = Arc::make_mut(&mut draft_arc);
        let result = f(data_mut);
        guard.1 = Some(draft_arc);
        result
    }

    /// 将草稿提交到已提交位置（替换），并清除草稿
    #[inline]
    pub fn apply(&self) {
        let mut guard = self.inner.data.write();
        if let Some(d) = guard.1.take() {
            guard.0 = d;
        }
    }

    /// 丢弃草稿（如果存在）
    #[inline]
    pub fn discard(&self) {
        let mut guard = self.inner.data.write();
        guard.1 = None;
    }

    /// 异步地修改已提交数据：把已提交数据克隆一份到本地交给异步闭包，闭包返回
    /// 新的 T（替换已提交数据）和业务返回值 R。
    ///
    /// 调用之间是串行的；提交之后若存在草稿，会通过 [`DraftRebase`] 把草稿重新
    /// 落到新的已提交数据上，避免随后的 [`Draft::apply`] 用过时快照覆盖本次改动。
    #[inline]
    pub async fn with_data_modify<F, Fut, R>(&self, f: F) -> Result<R, anyhow::Error>
    where
        T: Send + Sync + 'static + DraftRebase,
        F: FnOnce(T) -> Fut + Send,
        Fut: std::future::Future<Output = Result<(T, R), anyhow::Error>> + Send,
    {
        // 串行化，保证闭包看到的快照在提交前不会被另一次 with_data_modify 改写。
        let _permit = self.inner.data_modify_lock.lock().await;

        let local = {
            let guard = self.inner.data.read();
            (*guard.0).clone()
        };

        let (new_local, res) = f(local).await?;

        let mut guard = self.inner.data.write();
        guard.0 = Arc::new(new_local);
        if let Some(stale) = guard.1.take() {
            let mut rebased = (*guard.0).clone();
            stale.rebase_onto(&mut rebased);
            guard.1 = Some(Arc::new(rebased));
        }
        Ok(res)
    }
}

impl<T: Clone> Clone for Draft<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
