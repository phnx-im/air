// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/link_preview/link_preview.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _url = 'https://air.ms/blog/link-previews';

const _imageKey = Key('image');

const _banner = LinkPreviewData(
  url: _url,
  title: 'Link previews in Air',
  description: 'How a card is resolved by the sender and travels inside.',
  imageWidth: 1200,
  imageHeight: 630,
);

const _thumbnail = LinkPreviewData(
  url: 'https://example.com',
  title: 'Example Domain',
  description: 'This domain is for use in documentation examples.',
  imageWidth: 200,
  imageHeight: 200,
);

const _profile = LinkPreviewData(
  url: 'https://social.example/@alice',
  title: 'Alice',
  description: 'Writes about cryptography and gardening.',
  imageWidth: 1200,
  imageHeight: 630,
  kind: LinkPreviewKind.profile,
);

const _video = LinkPreviewData(
  url: 'https://video.example/watch/42',
  title: 'A short film about messaging',
  description: 'Twelve minutes on end-to-end encryption.',
  imageWidth: 1280,
  imageHeight: 720,
  kind: LinkPreviewKind.video,
);

const _music = LinkPreviewData(
  url: 'https://music.example/track/7',
  title: 'Quiet Protocol',
  description: 'Single by The Ciphersuites.',
  imageWidth: 200,
  imageHeight: 200,
  kind: LinkPreviewKind.music,
);

const _textOnly = LinkPreviewData(
  url: 'https://example.org/docs',
  title: 'Documentation without a picture',
  description: 'Plain text card.',
);

Widget _image(BoxFit fit) =>
    const ColoredBox(key: _imageKey, color: Colors.teal);

void main() {
  group('LinkPreview', () {
    Widget buildSubject(Widget child) => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        home: Scaffold(
          body: Padding(padding: const EdgeInsets.all(16), child: child),
        ),
      ),
    );

    Future<void> pumpCard(
      WidgetTester tester,
      LinkPreviewData? data, {
      bool withImage = true,
      VoidCallback? onTap,
    }) async {
      await tester.pumpWidget(
        buildSubject(
          Align(
            alignment: Alignment.topLeft,
            child: SizedBox(
              width: 360,
              child: LinkPreview(
                url: _url,
                data: data,
                imageBuilder: withImage ? _image : null,
                onTap: onTap,
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    Finder banner() => find.descendant(
      of: find.byType(LinkPreview),
      matching: find.byType(AspectRatio),
    );

    Finder image() => find.byKey(_imageKey);

    Finder playBadge() => find.byWidgetPredicate(
      (w) => w is AppIcon && w.type == AppIconType.play,
    );

    group('shape', () {
      LinkPreviewData sized(double width, double height) => LinkPreviewData(
        url: _url,
        title: 'Title',
        imageWidth: width,
        imageHeight: height,
      );

      test('a wide landscape image leads with a banner', () {
        expect(linkPreviewLeadsWithBanner(_banner), isTrue);
        expect(linkPreviewLeadsWithBanner(sized(300, 200)), isTrue);
      });

      test('a narrow image is a thumbnail', () {
        expect(linkPreviewLeadsWithBanner(sized(299, 150)), isFalse);
      });

      test('an aspect out of range is a thumbnail', () {
        expect(linkPreviewLeadsWithBanner(sized(1200, 1000)), isFalse);
        expect(linkPreviewLeadsWithBanner(sized(2000, 500)), isTrue);
        expect(linkPreviewLeadsWithBanner(sized(2001, 500)), isFalse);
      });

      test('a profile is a thumbnail however wide', () {
        expect(linkPreviewLeadsWithBanner(_profile), isFalse);
      });

      test('no dimensions is text only', () {
        expect(linkPreviewLeadsWithBanner(_textOnly), isFalse);
      });

      testWidgets('builds a banner for a wide landscape image', (tester) async {
        await pumpCard(tester, _banner);

        expect(banner(), findsOneWidget);
        expect(image(), findsOneWidget);
      });

      testWidgets('builds a thumbnail for a narrow image', (tester) async {
        await pumpCard(tester, _thumbnail);

        expect(banner(), findsNothing);
        expect(image(), findsOneWidget);
        expect(find.byType(ClipOval), findsNothing);
      });

      testWidgets('builds a round thumbnail for a profile', (tester) async {
        await pumpCard(tester, _profile);

        expect(banner(), findsNothing);
        expect(image(), findsOneWidget);
        expect(find.byType(ClipOval), findsOneWidget);
      });

      testWidgets('builds no image without dimensions', (tester) async {
        await pumpCard(tester, _textOnly);

        expect(banner(), findsNothing);
        expect(image(), findsNothing);
      });

      testWidgets('builds no image without an image builder', (tester) async {
        await pumpCard(tester, _banner, withImage: false);

        expect(banner(), findsNothing);
        expect(image(), findsNothing);
        expect(find.text(_banner.title), findsOneWidget);
      });
    });

    group('play badge', () {
      for (final data in [_video, _music]) {
        testWidgets('overlays the image of a ${data.kind.name} link', (
          tester,
        ) async {
          await pumpCard(tester, data);

          expect(playBadge(), findsOneWidget);
        });
      }

      for (final data in [_banner, _thumbnail, _profile]) {
        testWidgets('stays off a ${data.kind.name} link', (tester) async {
          await pumpCard(tester, data);

          expect(playBadge(), findsNothing);
        });
      }

      testWidgets('needs an image to sit on', (tester) async {
        await pumpCard(tester, _video, withImage: false);

        expect(playBadge(), findsNothing);
      });
    });

    testWidgets('shows title, description and the url as source', (
      tester,
    ) async {
      await pumpCard(tester, _thumbnail);

      expect(find.text(_thumbnail.title), findsOneWidget);
      expect(find.text(_thumbnail.description!), findsOneWidget);
      expect(find.text(_thumbnail.url), findsOneWidget);
    });

    testWidgets('drops the description line without one', (tester) async {
      await pumpCard(
        tester,
        const LinkPreviewData(url: _url, title: 'Title only'),
      );

      final texts = tester
          .widgetList<Text>(
            find.descendant(
              of: find.byType(LinkPreview),
              matching: find.byType(Text),
            ),
          )
          .map((t) => t.data)
          .toList();
      expect(texts, ['Title only', _url]);
    });

    testWidgets('shows the url while pending', (tester) async {
      await pumpCard(tester, null);

      expect(find.text(_url), findsOneWidget);
      expect(image(), findsNothing);
    });

    testWidgets('reports a tap', (tester) async {
      var taps = 0;
      await pumpCard(tester, _banner, onTap: () => taps++);

      await tester.tap(find.byType(LinkPreview));

      expect(taps, 1);
    });

    Widget shapes({required bool isSelf}) => SizedBox(
      width: 360,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 12,
        children: [
          for (final data in [
            _banner,
            _video,
            _thumbnail,
            _music,
            _profile,
            _textOnly,
          ])
            LinkPreview(
              url: data.url,
              data: data,
              imageBuilder: _image,
              isSelf: isSelf,
            ),
          Align(
            alignment: Alignment.centerLeft,
            child: LinkPreview(url: _url, isSelf: isSelf),
          ),
        ],
      ),
    );

    for (final (isSelf, name) in [(false, 'other'), (true, 'self')]) {
      testWidgets('renders every shape ($name)', (tester) async {
        sizeView(tester, const Size(400, 1080));
        await tester.pumpWidget(buildSubject(shapes(isSelf: isSelf)));
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/link_preview_$name.png'),
        );
      });
    }

    testWidgets('renders every shape (dark mode)', (tester) async {
      sizeView(tester, const Size(400, 1080));
      tester.platformDispatcher.platformBrightnessTestValue = .dark;
      addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

      await tester.pumpWidget(buildSubject(shapes(isSelf: false)));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/link_preview_dark_mode.png'),
      );
    });
  });
}
