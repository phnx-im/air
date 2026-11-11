// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{mem, sync::Arc};

use airprotos::client::link_preview::LinkPreviewKind;
use encoding_rs::Encoding;
use lol_html::{
    AsciiCompatibleEncoding, HtmlRewriter, Settings, element, end_tag, send::SendHandlerTypes, text,
};
use parking_lot::Mutex;
use reqwest::header;

pub(super) struct Page {
    pub(super) title: String,
    pub(super) description: String,
    /// Unresolved, relative to the final page url
    pub(super) image: Option<String>,
    pub(super) kind: LinkPreviewKind,
}

/// Metadata collected while parsing, raw and with entities.
#[derive(Default)]
struct Meta {
    og_title: Option<String>,
    title: Option<String>,
    og_description: Option<String>,
    description: Option<String>,
    og_image: Option<String>,
    twitter_image: Option<String>,
    og_type: Option<String>,
    /// `<body>` or `</head>` reached, all metadata is in the head
    done: bool,
}

impl Page {
    const MAX_BYTES: usize = 2 * 1024 * 1024;

    pub(super) async fn read(mut response: reqwest::Response) -> anyhow::Result<Option<Self>> {
        let charset = charset(&response);
        let meta = Arc::new(Mutex::new(Meta::default()));
        let mut rewriter = HtmlRewriter::new(Self::settings(&meta, charset), |_: &[u8]| {});

        let mut read = 0;
        while let Some(chunk) = response.chunk().await? {
            read += chunk.len();
            if Self::MAX_BYTES < read {
                return Ok(None);
            }
            rewriter.write(&chunk)?;
            if meta.lock().done {
                break;
            }
        }
        let meta = mem::take(&mut *meta.lock());
        Ok(meta.into_page())
    }

    fn settings(
        meta: &Arc<Mutex<Meta>>,
        charset: Option<&'static Encoding>,
    ) -> Settings<'static, 'static, SendHandlerTypes> {
        let on_meta = Arc::clone(meta);
        let on_title = Arc::clone(meta);
        let on_body = Arc::clone(meta);
        let on_head = Arc::clone(meta);
        let settings = Settings::new_send()
            .append_element_content_handler(element!("meta", move |el| {
                let key = el
                    .get_attribute("property")
                    .or_else(|| el.get_attribute("name"));
                if let (Some(key), Some(content)) = (key, el.get_attribute("content")) {
                    on_meta.lock().set(&key, content)
                }
                Ok(())
            }))
            .append_element_content_handler(text!("title", move |chunk| {
                on_title.lock().extend_title(chunk.as_str());
                Ok(())
            }))
            .append_element_content_handler(element!("body", move |_| {
                on_body.lock().done = true;
                Ok(())
            }))
            .append_element_content_handler(element!("head", move |el| {
                let on_head_end = Arc::clone(&on_head);
                el.on_end_tag(end_tag!(move |_| {
                    on_head_end.lock().done = true;
                    Ok(())
                }))
            }));
        match charset.and_then(AsciiCompatibleEncoding::new) {
            Some(encoding) => settings.with_encoding(encoding),
            None => settings.with_adjust_charset_on_meta_tag(true),
        }
    }
}

impl Meta {
    /// The body is read in chunks => title might be read in chunks
    fn extend_title(&mut self, chunk: &str) {
        if !self.done {
            self.title.get_or_insert_default().push_str(chunk);
        }
    }

    fn set(&mut self, key: &str, content: String) {
        if self.done {
            return;
        }
        let slot = match key.to_ascii_lowercase().as_str() {
            "og:title" => &mut self.og_title,
            "og:description" => &mut self.og_description,
            "description" => &mut self.description,
            "og:image" | "og:image:url" | "og:image:secure_url" => &mut self.og_image,
            "twitter:image" => &mut self.twitter_image,
            "og:type" => &mut self.og_type,
            _ => return,
        };
        slot.get_or_insert(content);
    }

    fn into_page(self) -> Option<Page> {
        let text = |s: Option<String>| {
            let s = html_escape::decode_html_entities(&s?).into_owned();
            (!s.trim().is_empty()).then_some(s)
        };
        let title = text(self.og_title).or_else(|| text(self.title))?;
        let description = text(self.og_description)
            .or_else(|| text(self.description))
            .unwrap_or_default();
        let image = text(self.og_image).or_else(|| text(self.twitter_image));
        let kind = match self.og_type.as_deref().map(str::to_ascii_lowercase) {
            Some(ty) if ty.starts_with("video") => LinkPreviewKind::Video,
            Some(ty) if ty.starts_with("music") => LinkPreviewKind::Music,
            Some(ty) if ty == "profile" => LinkPreviewKind::Profile,
            _ => LinkPreviewKind::Default,
        };
        Some(Page {
            title,
            description,
            image,
            kind,
        })
    }
}

fn charset(response: &reqwest::Response) -> Option<&'static Encoding> {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()?;
    let label = content_type.split(';').skip(1).find_map(|param| {
        let (name, value) = param.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then_some(value.trim())
    })?;
    Encoding::for_label(label.trim_matches('"').as_bytes())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    type Chunk = Result<Vec<u8>, io::Error>;

    fn response(content_type: &str, chunks: Vec<Chunk>) -> reqwest::Response {
        let body = reqwest::Body::wrap_stream(tokio_stream::iter(chunks));
        http::Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .body(body)
            .unwrap()
            .into()
    }

    async fn read_chunks(content_type: &str, chunks: Vec<Chunk>) -> Option<Page> {
        Page::read(response(content_type, chunks)).await.unwrap()
    }

    async fn read(html: &str) -> Option<Page> {
        read_chunks("text/html", vec![Ok(html.as_bytes().to_vec())]).await
    }

    #[tokio::test]
    async fn og_tags() {
        let page = read(
            r#"<html><head>
            <title>Title</title>
            <meta property="og:title" content="OG title">
            <meta property="og:description" content="OG description">
            <meta property="og:image" content="/image.png">
            <meta property="og:type" content="article">
            </head><body></body></html>"#,
        )
        .await
        .unwrap();
        assert_eq!(page.title, "OG title");
        assert_eq!(page.description, "OG description");
        assert_eq!(page.image.as_deref(), Some("/image.png"));
        assert_eq!(page.kind, LinkPreviewKind::Default);
    }

    #[tokio::test]
    async fn fallbacks() {
        let page = read(
            r#"<head>
            <meta property="og:title" content=" ">
            <meta property="og:description" content="">
            <meta property="og:image" content="">
            <title>Title</title>
            <meta name="description" content="Description">
            <meta name="twitter:image" content="https://a.org/t.png">
            </head>"#,
        )
        .await
        .unwrap();
        assert_eq!(page.title, "Title");
        assert_eq!(page.description, "Description");
        assert_eq!(page.image.as_deref(), Some("https://a.org/t.png"));
    }

    #[tokio::test]
    async fn first_value_wins() {
        let page = read(
            r#"<head>
            <meta property="og:title" content="First">
            <meta property="og:title" content="Second">
            <meta property="og:image:secure_url" content="https://a.org/1.png">
            <meta property="og:image" content="https://a.org/2.png">
            </head>"#,
        )
        .await
        .unwrap();
        assert_eq!(page.title, "First");
        assert_eq!(page.image.as_deref(), Some("https://a.org/1.png"));
    }

    #[tokio::test]
    async fn keys_are_case_insensitive() {
        let page = read(r#"<meta property="OG:Title" content="Title">"#)
            .await
            .unwrap();
        assert_eq!(page.title, "Title");
    }

    #[tokio::test]
    async fn entities_are_decoded_once() {
        let page = read(
            r#"<head>
            <title>A &amp;amp; B</title>
            <meta property="og:description" content="&lt;b&gt; &quot;x&quot;">
            <meta property="og:image" content="https://a.org/i.png?x=1&amp;y=2">
            </head>"#,
        )
        .await
        .unwrap();
        assert_eq!(page.title, "A &amp; B");
        assert_eq!(page.description, r#"<b> "x""#);
        assert_eq!(page.image.as_deref(), Some("https://a.org/i.png?x=1&y=2"));
    }

    #[tokio::test]
    async fn kinds() {
        for (og_type, kind) in [
            ("video.movie", LinkPreviewKind::Video),
            ("music.song", LinkPreviewKind::Music),
            ("Profile", LinkPreviewKind::Profile),
            ("website", LinkPreviewKind::Default),
        ] {
            let html = format!(r#"<title>t</title><meta property="og:type" content="{og_type}">"#);
            assert_eq!(read(&html).await.unwrap().kind, kind, "{og_type}");
        }
    }

    #[tokio::test]
    async fn no_title() {
        assert!(read("<head></head><body>text</body>").await.is_none());
        assert!(read("<title>  </title>").await.is_none());
    }

    #[tokio::test]
    async fn title_split_across_chunks() {
        let chunks = vec![
            Ok(b"<title>Link ".to_vec()),
            Ok(b"preview</title>".to_vec()),
        ];
        let page = read_chunks("text/html", chunks).await.unwrap();
        assert_eq!(page.title, "Hello");
    }

    #[tokio::test]
    async fn stops_at_body() {
        // The second chunk would fail if it were read
        let chunks = vec![
            Ok(b"<head><title>Title</title></head><body><p>x".to_vec()),
            Err(io::Error::other("read past body")),
        ];
        let page = read_chunks("text/html", chunks).await.unwrap();
        assert_eq!(page.title, "Title");
    }

    #[tokio::test]
    async fn stops_at_head_end() {
        let chunks = vec![
            Ok(b"<head><title>Title</title></head>x".to_vec()),
            Err(io::Error::other("read past head")),
        ];
        let page = read_chunks("text/html", chunks).await.unwrap();
        assert_eq!(page.title, "Title");
    }

    #[tokio::test]
    async fn ignores_meta_after_head() {
        let page = read(
            r#"<head><title>Title</title></head>
            <body><meta property="og:title" content="Late"></body>"#,
        )
        .await
        .unwrap();
        assert_eq!(page.title, "Title");
    }

    #[tokio::test]
    async fn charset_from_header() {
        let (html, _, _) = encoding_rs::WINDOWS_1251.encode("<title>Привет</title>");
        for content_type in [
            "text/html; charset=windows-1251",
            r#"text/html; Charset="windows-1251""#,
        ] {
            let chunks = vec![Ok(html.to_vec())];
            let page = read_chunks(content_type, chunks).await.unwrap();
            assert_eq!(page.title, "Привет", "{content_type}");
        }
    }

    #[tokio::test]
    async fn charset_from_meta() {
        let (html, _, _) = encoding_rs::WINDOWS_1251
            .encode(r#"<meta charset="windows-1251"><title>Привет</title>"#);
        let page = read_chunks("text/html", vec![Ok(html.to_vec())])
            .await
            .unwrap();
        assert_eq!(page.title, "Привет");
    }

    #[tokio::test]
    async fn over_size_cap() {
        let html = format!("<head><title>t</title>{}", " ".repeat(Page::MAX_BYTES));
        assert!(read(&html).await.is_none());
    }
}
