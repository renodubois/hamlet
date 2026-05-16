use hamlet_client_iced::embeds::{
    EmbedImageCache, EmbedImageStatus, EmbedRenderMode, embed_render_mode,
};
use hamlet_client_iced::protocol::Embed;

#[test]
fn photo_and_image_embeds_render_as_native_image_previews_when_supported() {
    let photo = embed("photo", Some("https://cdn.example.test/photo.jpg"), None);
    let image = embed("image", Some("/uploads/previews/image.png"), None);

    assert_eq!(
        embed_render_mode(&photo),
        EmbedRenderMode::NativeImagePreview
    );
    assert_eq!(
        embed_render_mode(&image),
        EmbedRenderMode::NativeImagePreview
    );
}

#[test]
fn rich_and_video_iframe_embeds_fall_back_to_external_open_cards() {
    let rich = embed(
        "rich",
        Some("https://cdn.example.test/thumb.jpg"),
        Some("https://player.example.test/embed/1"),
    );
    let video = embed(
        "video",
        Some("https://cdn.example.test/video.jpg"),
        Some("https://video.example.test/embed/1"),
    );

    assert_eq!(embed_render_mode(&rich), EmbedRenderMode::ExternalOpenCard);
    assert_eq!(embed_render_mode(&video), EmbedRenderMode::ExternalOpenCard);
}

#[test]
fn link_embeds_render_as_preview_cards_with_optional_image_data() {
    let link = embed("link", Some("https://cdn.example.test/preview.jpg"), None);

    assert_eq!(embed_render_mode(&link), EmbedRenderMode::LinkCard);
    assert!(link.image_url.is_some());
}

#[test]
fn embed_image_cache_tracks_preview_image_data() {
    let mut cache = EmbedImageCache::default();
    let request = match cache.begin_load("http://localhost:3030", Some("/uploads/previews/42.png"))
    {
        Some(request) => request,
        None => panic!("first preview image load should queue"),
    };

    assert_eq!(request.url, "http://localhost:3030/uploads/previews/42.png");
    assert!(
        cache
            .status_for(&request.url)
            .is_some_and(EmbedImageStatus::is_loading)
    );
    assert_eq!(
        cache.begin_load("http://localhost:3030", Some("/uploads/previews/42.png")),
        None
    );

    cache.complete_load(request.url.clone(), Ok(vec![1, 2, 3, 4]));

    assert!(
        cache
            .status_for(&request.url)
            .is_some_and(EmbedImageStatus::is_loaded)
    );
    assert!(
        cache
            .handle_for("http://localhost:3030", Some("/uploads/previews/42.png"))
            .is_some()
    );
}

fn embed(kind: &str, image_url: Option<&str>, iframe_url: Option<&str>) -> Embed {
    Embed {
        id: 1,
        message_id: 10,
        url: "https://example.test/post".to_string(),
        title: Some("Example".to_string()),
        description: Some("Preview description".to_string()),
        image_url: image_url.map(str::to_string),
        site_name: Some("Example Site".to_string()),
        embed_type: kind.to_string(),
        iframe_url: iframe_url.map(str::to_string),
        iframe_width: Some(640),
        iframe_height: Some(360),
    }
}
