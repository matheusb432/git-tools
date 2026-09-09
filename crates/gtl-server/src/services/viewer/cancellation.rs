use gtl_application::viewer::rows::ViewerWorkCancellation;

pub(super) struct CancelOnDrop(pub(super) ViewerWorkCancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
