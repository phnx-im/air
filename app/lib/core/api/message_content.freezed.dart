// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'message_content.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UiAttachment {

 AttachmentId get attachmentId; String get filename; String get contentType; String? get description; int get size; UiImageMetadata? get imageMetadata;
/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiAttachmentCopyWith<UiAttachment> get copyWith => _$UiAttachmentCopyWithImpl<UiAttachment>(this as UiAttachment, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiAttachment;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachment&&(identical(other.attachmentId, _this.attachmentId) || other.attachmentId == _this.attachmentId)&&(identical(other.filename, _this.filename) || other.filename == _this.filename)&&(identical(other.contentType, _this.contentType) || other.contentType == _this.contentType)&&(identical(other.description, _this.description) || other.description == _this.description)&&(identical(other.size, _this.size) || other.size == _this.size)&&(identical(other.imageMetadata, _this.imageMetadata) || other.imageMetadata == _this.imageMetadata));
}


@override
int get hashCode {
  final _this = this as UiAttachment;
  return Object.hash(runtimeType,_this.attachmentId,_this.filename,_this.contentType,_this.description,_this.size,_this.imageMetadata);
}

@override
String toString() {
  final _this = this as UiAttachment;
  return 'UiAttachment(attachmentId: ${_this.attachmentId}, filename: ${_this.filename}, contentType: ${_this.contentType}, description: ${_this.description}, size: ${_this.size}, imageMetadata: ${_this.imageMetadata})';
}


}

/// @nodoc
abstract mixin class $UiAttachmentCopyWith<$Res>  {
  factory $UiAttachmentCopyWith(UiAttachment value, $Res Function(UiAttachment) _then) = _$UiAttachmentCopyWithImpl;
@useResult
$Res call({
 AttachmentId attachmentId, String filename, String contentType, String? description, int size, UiImageMetadata? imageMetadata
});


$UiImageMetadataCopyWith<$Res>? get imageMetadata;

}
/// @nodoc
class _$UiAttachmentCopyWithImpl<$Res>
    implements $UiAttachmentCopyWith<$Res> {
  _$UiAttachmentCopyWithImpl(this._self, this._then);

  final UiAttachment _self;
  final $Res Function(UiAttachment) _then;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachmentId = null,Object? filename = null,Object? contentType = null,Object? description = freezed,Object? size = null,Object? imageMetadata = freezed,}) {
  return _then(UiAttachment(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,description: freezed == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String?,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int,imageMetadata: freezed == imageMetadata ? _self.imageMetadata : imageMetadata // ignore: cast_nullable_to_non_nullable
as UiImageMetadata?,
  ));
}
/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<$Res>? get imageMetadata {
    if (_self.imageMetadata == null) {
    return null;
  }

  return $UiImageMetadataCopyWith<$Res>(_self.imageMetadata!, (value) {
    return _then(_self.copyWith(imageMetadata: value));
  });
}
}



/// @nodoc


class _UiAttachment implements UiAttachment {
  const _UiAttachment({required this.attachmentId, required this.filename, required this.contentType, this.description, required this.size, this.imageMetadata});
  

@override final  AttachmentId attachmentId;
@override final  String filename;
@override final  String contentType;
@override final  String? description;
@override final  int size;
@override final  UiImageMetadata? imageMetadata;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiAttachmentCopyWith<_UiAttachment> get copyWith => __$UiAttachmentCopyWithImpl<_UiAttachment>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiAttachment&&(identical(other.attachmentId, attachmentId) || other.attachmentId == attachmentId)&&(identical(other.filename, filename) || other.filename == filename)&&(identical(other.contentType, contentType) || other.contentType == contentType)&&(identical(other.description, description) || other.description == description)&&(identical(other.size, size) || other.size == size)&&(identical(other.imageMetadata, imageMetadata) || other.imageMetadata == imageMetadata));
}


@override
int get hashCode {
    return Object.hash(runtimeType,attachmentId,filename,contentType,description,size,imageMetadata);
}

@override
String toString() {
    return 'UiAttachment(attachmentId: $attachmentId, filename: $filename, contentType: $contentType, description: $description, size: $size, imageMetadata: $imageMetadata)';
}


}

/// @nodoc
abstract mixin class _$UiAttachmentCopyWith<$Res> implements $UiAttachmentCopyWith<$Res> {
  factory _$UiAttachmentCopyWith(_UiAttachment value, $Res Function(_UiAttachment) _then) = __$UiAttachmentCopyWithImpl;
@override @useResult
$Res call({
 AttachmentId attachmentId, String filename, String contentType, String? description, int size, UiImageMetadata? imageMetadata
});


@override $UiImageMetadataCopyWith<$Res>? get imageMetadata;

}
/// @nodoc
class __$UiAttachmentCopyWithImpl<$Res>
    implements _$UiAttachmentCopyWith<$Res> {
  __$UiAttachmentCopyWithImpl(this._self, this._then);

  final _UiAttachment _self;
  final $Res Function(_UiAttachment) _then;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachmentId = null,Object? filename = null,Object? contentType = null,Object? description = freezed,Object? size = null,Object? imageMetadata = freezed,}) {
  return _then(_UiAttachment(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,description: freezed == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String?,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int,imageMetadata: freezed == imageMetadata ? _self.imageMetadata : imageMetadata // ignore: cast_nullable_to_non_nullable
as UiImageMetadata?,
  ));
}

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<$Res>? get imageMetadata {
    if (_self.imageMetadata == null) {
    return null;
  }

  return $UiImageMetadataCopyWith<$Res>(_self.imageMetadata!, (value) {
    return _then(_self.copyWith(imageMetadata: value));
  });
}
}

/// @nodoc
mixin _$UiComposerLinkPreview {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiComposerLinkPreview);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiComposerLinkPreview()';
}


}

/// @nodoc
class $UiComposerLinkPreviewCopyWith<$Res>  {
$UiComposerLinkPreviewCopyWith(UiComposerLinkPreview _, $Res Function(UiComposerLinkPreview) __);
}



/// @nodoc


class UiComposerLinkPreview_Loading extends UiComposerLinkPreview {
  const UiComposerLinkPreview_Loading({required this.url}): super._();
  

 final  String url;

/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiComposerLinkPreview_LoadingCopyWith<UiComposerLinkPreview_Loading> get copyWith => _$UiComposerLinkPreview_LoadingCopyWithImpl<UiComposerLinkPreview_Loading>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiComposerLinkPreview_Loading&&(identical(other.url, url) || other.url == url));
}


@override
int get hashCode {
    return Object.hash(runtimeType,url);
}

@override
String toString() {
    return 'UiComposerLinkPreview.loading(url: $url)';
}


}

/// @nodoc
abstract mixin class $UiComposerLinkPreview_LoadingCopyWith<$Res> implements $UiComposerLinkPreviewCopyWith<$Res> {
  factory $UiComposerLinkPreview_LoadingCopyWith(UiComposerLinkPreview_Loading value, $Res Function(UiComposerLinkPreview_Loading) _then) = _$UiComposerLinkPreview_LoadingCopyWithImpl;
@useResult
$Res call({
 String url
});




}
/// @nodoc
class _$UiComposerLinkPreview_LoadingCopyWithImpl<$Res>
    implements $UiComposerLinkPreview_LoadingCopyWith<$Res> {
  _$UiComposerLinkPreview_LoadingCopyWithImpl(this._self, this._then);

  final UiComposerLinkPreview_Loading _self;
  final $Res Function(UiComposerLinkPreview_Loading) _then;

/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? url = null,}) {
  return _then(UiComposerLinkPreview_Loading(
url: null == url ? _self.url : url // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiComposerLinkPreview_Ready extends UiComposerLinkPreview {
  const UiComposerLinkPreview_Ready({required this.card, this.image}): super._();
  

 final  UiLinkPreviewCard card;
/// Dimensions and blurhash, no bytes to keep the state changes small
 final  UiImageMetadata? image;

/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiComposerLinkPreview_ReadyCopyWith<UiComposerLinkPreview_Ready> get copyWith => _$UiComposerLinkPreview_ReadyCopyWithImpl<UiComposerLinkPreview_Ready>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiComposerLinkPreview_Ready&&(identical(other.card, card) || other.card == card)&&(identical(other.image, image) || other.image == image));
}


@override
int get hashCode {
    return Object.hash(runtimeType,card,image);
}

@override
String toString() {
    return 'UiComposerLinkPreview.ready(card: $card, image: $image)';
}


}

/// @nodoc
abstract mixin class $UiComposerLinkPreview_ReadyCopyWith<$Res> implements $UiComposerLinkPreviewCopyWith<$Res> {
  factory $UiComposerLinkPreview_ReadyCopyWith(UiComposerLinkPreview_Ready value, $Res Function(UiComposerLinkPreview_Ready) _then) = _$UiComposerLinkPreview_ReadyCopyWithImpl;
@useResult
$Res call({
 UiLinkPreviewCard card, UiImageMetadata? image
});


$UiLinkPreviewCardCopyWith<$Res> get card;$UiImageMetadataCopyWith<$Res>? get image;

}
/// @nodoc
class _$UiComposerLinkPreview_ReadyCopyWithImpl<$Res>
    implements $UiComposerLinkPreview_ReadyCopyWith<$Res> {
  _$UiComposerLinkPreview_ReadyCopyWithImpl(this._self, this._then);

  final UiComposerLinkPreview_Ready _self;
  final $Res Function(UiComposerLinkPreview_Ready) _then;

/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? card = null,Object? image = freezed,}) {
  return _then(UiComposerLinkPreview_Ready(
card: null == card ? _self.card : card // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewCard,image: freezed == image ? _self.image : image // ignore: cast_nullable_to_non_nullable
as UiImageMetadata?,
  ));
}

/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewCardCopyWith<$Res> get card {
  
  return $UiLinkPreviewCardCopyWith<$Res>(_self.card, (value) {
    return _then(_self.copyWith(card: value));
  });
}/// Create a copy of UiComposerLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<$Res>? get image {
    if (_self.image == null) {
    return null;
  }

  return $UiImageMetadataCopyWith<$Res>(_self.image!, (value) {
    return _then(_self.copyWith(image: value));
  });
}
}

/// @nodoc
mixin _$UiImageMetadata {

 String? get blurhash; int get width; int get height; bool? get isAnimated;
/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<UiImageMetadata> get copyWith => _$UiImageMetadataCopyWithImpl<UiImageMetadata>(this as UiImageMetadata, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiImageMetadata;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiImageMetadata&&(identical(other.blurhash, _this.blurhash) || other.blurhash == _this.blurhash)&&(identical(other.width, _this.width) || other.width == _this.width)&&(identical(other.height, _this.height) || other.height == _this.height)&&(identical(other.isAnimated, _this.isAnimated) || other.isAnimated == _this.isAnimated));
}


@override
int get hashCode {
  final _this = this as UiImageMetadata;
  return Object.hash(runtimeType,_this.blurhash,_this.width,_this.height,_this.isAnimated);
}

@override
String toString() {
  final _this = this as UiImageMetadata;
  return 'UiImageMetadata(blurhash: ${_this.blurhash}, width: ${_this.width}, height: ${_this.height}, isAnimated: ${_this.isAnimated})';
}


}

/// @nodoc
abstract mixin class $UiImageMetadataCopyWith<$Res>  {
  factory $UiImageMetadataCopyWith(UiImageMetadata value, $Res Function(UiImageMetadata) _then) = _$UiImageMetadataCopyWithImpl;
@useResult
$Res call({
 String? blurhash, int width, int height, bool? isAnimated
});




}
/// @nodoc
class _$UiImageMetadataCopyWithImpl<$Res>
    implements $UiImageMetadataCopyWith<$Res> {
  _$UiImageMetadataCopyWithImpl(this._self, this._then);

  final UiImageMetadata _self;
  final $Res Function(UiImageMetadata) _then;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? blurhash = freezed,Object? width = null,Object? height = null,Object? isAnimated = freezed,}) {
  return _then(UiImageMetadata(
blurhash: freezed == blurhash ? _self.blurhash : blurhash // ignore: cast_nullable_to_non_nullable
as String?,width: null == width ? _self.width : width // ignore: cast_nullable_to_non_nullable
as int,height: null == height ? _self.height : height // ignore: cast_nullable_to_non_nullable
as int,isAnimated: freezed == isAnimated ? _self.isAnimated : isAnimated // ignore: cast_nullable_to_non_nullable
as bool?,
  ));
}

}



/// @nodoc


class _UiImageMetadata implements UiImageMetadata {
  const _UiImageMetadata({this.blurhash, required this.width, required this.height, this.isAnimated});
  

@override final  String? blurhash;
@override final  int width;
@override final  int height;
@override final  bool? isAnimated;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiImageMetadataCopyWith<_UiImageMetadata> get copyWith => __$UiImageMetadataCopyWithImpl<_UiImageMetadata>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiImageMetadata&&(identical(other.blurhash, blurhash) || other.blurhash == blurhash)&&(identical(other.width, width) || other.width == width)&&(identical(other.height, height) || other.height == height)&&(identical(other.isAnimated, isAnimated) || other.isAnimated == isAnimated));
}


@override
int get hashCode {
    return Object.hash(runtimeType,blurhash,width,height,isAnimated);
}

@override
String toString() {
    return 'UiImageMetadata(blurhash: $blurhash, width: $width, height: $height, isAnimated: $isAnimated)';
}


}

/// @nodoc
abstract mixin class _$UiImageMetadataCopyWith<$Res> implements $UiImageMetadataCopyWith<$Res> {
  factory _$UiImageMetadataCopyWith(_UiImageMetadata value, $Res Function(_UiImageMetadata) _then) = __$UiImageMetadataCopyWithImpl;
@override @useResult
$Res call({
 String? blurhash, int width, int height, bool? isAnimated
});




}
/// @nodoc
class __$UiImageMetadataCopyWithImpl<$Res>
    implements _$UiImageMetadataCopyWith<$Res> {
  __$UiImageMetadataCopyWithImpl(this._self, this._then);

  final _UiImageMetadata _self;
  final $Res Function(_UiImageMetadata) _then;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? blurhash = freezed,Object? width = null,Object? height = null,Object? isAnimated = freezed,}) {
  return _then(_UiImageMetadata(
blurhash: freezed == blurhash ? _self.blurhash : blurhash // ignore: cast_nullable_to_non_nullable
as String?,width: null == width ? _self.width : width // ignore: cast_nullable_to_non_nullable
as int,height: null == height ? _self.height : height // ignore: cast_nullable_to_non_nullable
as int,isAnimated: freezed == isAnimated ? _self.isAnimated : isAnimated // ignore: cast_nullable_to_non_nullable
as bool?,
  ));
}


}

/// @nodoc
mixin _$UiLinkPreview {

 UiLinkPreviewCard get card; UiLinkPreviewImage? get image;
/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiLinkPreviewCopyWith<UiLinkPreview> get copyWith => _$UiLinkPreviewCopyWithImpl<UiLinkPreview>(this as UiLinkPreview, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiLinkPreview;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiLinkPreview&&(identical(other.card, _this.card) || other.card == _this.card)&&(identical(other.image, _this.image) || other.image == _this.image));
}


@override
int get hashCode {
  final _this = this as UiLinkPreview;
  return Object.hash(runtimeType,_this.card,_this.image);
}

@override
String toString() {
  final _this = this as UiLinkPreview;
  return 'UiLinkPreview(card: ${_this.card}, image: ${_this.image})';
}


}

/// @nodoc
abstract mixin class $UiLinkPreviewCopyWith<$Res>  {
  factory $UiLinkPreviewCopyWith(UiLinkPreview value, $Res Function(UiLinkPreview) _then) = _$UiLinkPreviewCopyWithImpl;
@useResult
$Res call({
 UiLinkPreviewCard card, UiLinkPreviewImage? image
});


$UiLinkPreviewCardCopyWith<$Res> get card;$UiLinkPreviewImageCopyWith<$Res>? get image;

}
/// @nodoc
class _$UiLinkPreviewCopyWithImpl<$Res>
    implements $UiLinkPreviewCopyWith<$Res> {
  _$UiLinkPreviewCopyWithImpl(this._self, this._then);

  final UiLinkPreview _self;
  final $Res Function(UiLinkPreview) _then;

/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? card = null,Object? image = freezed,}) {
  return _then(UiLinkPreview(
card: null == card ? _self.card : card // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewCard,image: freezed == image ? _self.image : image // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewImage?,
  ));
}
/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewCardCopyWith<$Res> get card {
  
  return $UiLinkPreviewCardCopyWith<$Res>(_self.card, (value) {
    return _then(_self.copyWith(card: value));
  });
}/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewImageCopyWith<$Res>? get image {
    if (_self.image == null) {
    return null;
  }

  return $UiLinkPreviewImageCopyWith<$Res>(_self.image!, (value) {
    return _then(_self.copyWith(image: value));
  });
}
}



/// @nodoc


class _UiLinkPreview implements UiLinkPreview {
  const _UiLinkPreview({required this.card, this.image});
  

@override final  UiLinkPreviewCard card;
@override final  UiLinkPreviewImage? image;

/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiLinkPreviewCopyWith<_UiLinkPreview> get copyWith => __$UiLinkPreviewCopyWithImpl<_UiLinkPreview>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiLinkPreview&&(identical(other.card, card) || other.card == card)&&(identical(other.image, image) || other.image == image));
}


@override
int get hashCode {
    return Object.hash(runtimeType,card,image);
}

@override
String toString() {
    return 'UiLinkPreview(card: $card, image: $image)';
}


}

/// @nodoc
abstract mixin class _$UiLinkPreviewCopyWith<$Res> implements $UiLinkPreviewCopyWith<$Res> {
  factory _$UiLinkPreviewCopyWith(_UiLinkPreview value, $Res Function(_UiLinkPreview) _then) = __$UiLinkPreviewCopyWithImpl;
@override @useResult
$Res call({
 UiLinkPreviewCard card, UiLinkPreviewImage? image
});


@override $UiLinkPreviewCardCopyWith<$Res> get card;@override $UiLinkPreviewImageCopyWith<$Res>? get image;

}
/// @nodoc
class __$UiLinkPreviewCopyWithImpl<$Res>
    implements _$UiLinkPreviewCopyWith<$Res> {
  __$UiLinkPreviewCopyWithImpl(this._self, this._then);

  final _UiLinkPreview _self;
  final $Res Function(_UiLinkPreview) _then;

/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? card = null,Object? image = freezed,}) {
  return _then(_UiLinkPreview(
card: null == card ? _self.card : card // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewCard,image: freezed == image ? _self.image : image // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewImage?,
  ));
}

/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewCardCopyWith<$Res> get card {
  
  return $UiLinkPreviewCardCopyWith<$Res>(_self.card, (value) {
    return _then(_self.copyWith(card: value));
  });
}/// Create a copy of UiLinkPreview
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewImageCopyWith<$Res>? get image {
    if (_self.image == null) {
    return null;
  }

  return $UiLinkPreviewImageCopyWith<$Res>(_self.image!, (value) {
    return _then(_self.copyWith(image: value));
  });
}
}

/// @nodoc
mixin _$UiLinkPreviewCard {

 String get url; String get title; String get description; UiLinkPreviewKind get kind;
/// Create a copy of UiLinkPreviewCard
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiLinkPreviewCardCopyWith<UiLinkPreviewCard> get copyWith => _$UiLinkPreviewCardCopyWithImpl<UiLinkPreviewCard>(this as UiLinkPreviewCard, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiLinkPreviewCard;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiLinkPreviewCard&&(identical(other.url, _this.url) || other.url == _this.url)&&(identical(other.title, _this.title) || other.title == _this.title)&&(identical(other.description, _this.description) || other.description == _this.description)&&(identical(other.kind, _this.kind) || other.kind == _this.kind));
}


@override
int get hashCode {
  final _this = this as UiLinkPreviewCard;
  return Object.hash(runtimeType,_this.url,_this.title,_this.description,_this.kind);
}

@override
String toString() {
  final _this = this as UiLinkPreviewCard;
  return 'UiLinkPreviewCard(url: ${_this.url}, title: ${_this.title}, description: ${_this.description}, kind: ${_this.kind})';
}


}

/// @nodoc
abstract mixin class $UiLinkPreviewCardCopyWith<$Res>  {
  factory $UiLinkPreviewCardCopyWith(UiLinkPreviewCard value, $Res Function(UiLinkPreviewCard) _then) = _$UiLinkPreviewCardCopyWithImpl;
@useResult
$Res call({
 String url, String title, String description, UiLinkPreviewKind kind
});




}
/// @nodoc
class _$UiLinkPreviewCardCopyWithImpl<$Res>
    implements $UiLinkPreviewCardCopyWith<$Res> {
  _$UiLinkPreviewCardCopyWithImpl(this._self, this._then);

  final UiLinkPreviewCard _self;
  final $Res Function(UiLinkPreviewCard) _then;

/// Create a copy of UiLinkPreviewCard
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? url = null,Object? title = null,Object? description = null,Object? kind = null,}) {
  return _then(UiLinkPreviewCard(
url: null == url ? _self.url : url // ignore: cast_nullable_to_non_nullable
as String,title: null == title ? _self.title : title // ignore: cast_nullable_to_non_nullable
as String,description: null == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewKind,
  ));
}

}



/// @nodoc


class _UiLinkPreviewCard implements UiLinkPreviewCard {
  const _UiLinkPreviewCard({required this.url, required this.title, required this.description, required this.kind});
  

@override final  String url;
@override final  String title;
@override final  String description;
@override final  UiLinkPreviewKind kind;

/// Create a copy of UiLinkPreviewCard
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiLinkPreviewCardCopyWith<_UiLinkPreviewCard> get copyWith => __$UiLinkPreviewCardCopyWithImpl<_UiLinkPreviewCard>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiLinkPreviewCard&&(identical(other.url, url) || other.url == url)&&(identical(other.title, title) || other.title == title)&&(identical(other.description, description) || other.description == description)&&(identical(other.kind, kind) || other.kind == kind));
}


@override
int get hashCode {
    return Object.hash(runtimeType,url,title,description,kind);
}

@override
String toString() {
    return 'UiLinkPreviewCard(url: $url, title: $title, description: $description, kind: $kind)';
}


}

/// @nodoc
abstract mixin class _$UiLinkPreviewCardCopyWith<$Res> implements $UiLinkPreviewCardCopyWith<$Res> {
  factory _$UiLinkPreviewCardCopyWith(_UiLinkPreviewCard value, $Res Function(_UiLinkPreviewCard) _then) = __$UiLinkPreviewCardCopyWithImpl;
@override @useResult
$Res call({
 String url, String title, String description, UiLinkPreviewKind kind
});




}
/// @nodoc
class __$UiLinkPreviewCardCopyWithImpl<$Res>
    implements _$UiLinkPreviewCardCopyWith<$Res> {
  __$UiLinkPreviewCardCopyWithImpl(this._self, this._then);

  final _UiLinkPreviewCard _self;
  final $Res Function(_UiLinkPreviewCard) _then;

/// Create a copy of UiLinkPreviewCard
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? url = null,Object? title = null,Object? description = null,Object? kind = null,}) {
  return _then(_UiLinkPreviewCard(
url: null == url ? _self.url : url // ignore: cast_nullable_to_non_nullable
as String,title: null == title ? _self.title : title // ignore: cast_nullable_to_non_nullable
as String,description: null == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as UiLinkPreviewKind,
  ));
}


}

/// @nodoc
mixin _$UiLinkPreviewImage {

 UiAttachment get attachment; String? get alt;
/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiLinkPreviewImageCopyWith<UiLinkPreviewImage> get copyWith => _$UiLinkPreviewImageCopyWithImpl<UiLinkPreviewImage>(this as UiLinkPreviewImage, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiLinkPreviewImage;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiLinkPreviewImage&&(identical(other.attachment, _this.attachment) || other.attachment == _this.attachment)&&(identical(other.alt, _this.alt) || other.alt == _this.alt));
}


@override
int get hashCode {
  final _this = this as UiLinkPreviewImage;
  return Object.hash(runtimeType,_this.attachment,_this.alt);
}

@override
String toString() {
  final _this = this as UiLinkPreviewImage;
  return 'UiLinkPreviewImage(attachment: ${_this.attachment}, alt: ${_this.alt})';
}


}

/// @nodoc
abstract mixin class $UiLinkPreviewImageCopyWith<$Res>  {
  factory $UiLinkPreviewImageCopyWith(UiLinkPreviewImage value, $Res Function(UiLinkPreviewImage) _then) = _$UiLinkPreviewImageCopyWithImpl;
@useResult
$Res call({
 UiAttachment attachment, String? alt
});


$UiAttachmentCopyWith<$Res> get attachment;

}
/// @nodoc
class _$UiLinkPreviewImageCopyWithImpl<$Res>
    implements $UiLinkPreviewImageCopyWith<$Res> {
  _$UiLinkPreviewImageCopyWithImpl(this._self, this._then);

  final UiLinkPreviewImage _self;
  final $Res Function(UiLinkPreviewImage) _then;

/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachment = null,Object? alt = freezed,}) {
  return _then(UiLinkPreviewImage(
attachment: null == attachment ? _self.attachment : attachment // ignore: cast_nullable_to_non_nullable
as UiAttachment,alt: freezed == alt ? _self.alt : alt // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}
/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiAttachmentCopyWith<$Res> get attachment {
  
  return $UiAttachmentCopyWith<$Res>(_self.attachment, (value) {
    return _then(_self.copyWith(attachment: value));
  });
}
}



/// @nodoc


class _UiLinkPreviewImage implements UiLinkPreviewImage {
  const _UiLinkPreviewImage({required this.attachment, this.alt});
  

@override final  UiAttachment attachment;
@override final  String? alt;

/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiLinkPreviewImageCopyWith<_UiLinkPreviewImage> get copyWith => __$UiLinkPreviewImageCopyWithImpl<_UiLinkPreviewImage>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiLinkPreviewImage&&(identical(other.attachment, attachment) || other.attachment == attachment)&&(identical(other.alt, alt) || other.alt == alt));
}


@override
int get hashCode {
    return Object.hash(runtimeType,attachment,alt);
}

@override
String toString() {
    return 'UiLinkPreviewImage(attachment: $attachment, alt: $alt)';
}


}

/// @nodoc
abstract mixin class _$UiLinkPreviewImageCopyWith<$Res> implements $UiLinkPreviewImageCopyWith<$Res> {
  factory _$UiLinkPreviewImageCopyWith(_UiLinkPreviewImage value, $Res Function(_UiLinkPreviewImage) _then) = __$UiLinkPreviewImageCopyWithImpl;
@override @useResult
$Res call({
 UiAttachment attachment, String? alt
});


@override $UiAttachmentCopyWith<$Res> get attachment;

}
/// @nodoc
class __$UiLinkPreviewImageCopyWithImpl<$Res>
    implements _$UiLinkPreviewImageCopyWith<$Res> {
  __$UiLinkPreviewImageCopyWithImpl(this._self, this._then);

  final _UiLinkPreviewImage _self;
  final $Res Function(_UiLinkPreviewImage) _then;

/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachment = null,Object? alt = freezed,}) {
  return _then(_UiLinkPreviewImage(
attachment: null == attachment ? _self.attachment : attachment // ignore: cast_nullable_to_non_nullable
as UiAttachment,alt: freezed == alt ? _self.alt : alt // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}

/// Create a copy of UiLinkPreviewImage
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiAttachmentCopyWith<$Res> get attachment {
  
  return $UiAttachmentCopyWith<$Res>(_self.attachment, (value) {
    return _then(_self.copyWith(attachment: value));
  });
}
}

/// @nodoc
mixin _$UiMimiContent {

 String? get plainBody; String? get previewText; Uint8List? get replaces; Uint8List get topicId; Uint8List? get inReplyTo; MessageContent? get content; List<UiAttachment> get attachments; UiLinkPreview? get linkPreview;
/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiMimiContentCopyWith<UiMimiContent> get copyWith => _$UiMimiContentCopyWithImpl<UiMimiContent>(this as UiMimiContent, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiMimiContent;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiMimiContent&&(identical(other.plainBody, _this.plainBody) || other.plainBody == _this.plainBody)&&(identical(other.previewText, _this.previewText) || other.previewText == _this.previewText)&&const DeepCollectionEquality().equals(other.replaces, _this.replaces)&&const DeepCollectionEquality().equals(other.topicId, _this.topicId)&&const DeepCollectionEquality().equals(other.inReplyTo, _this.inReplyTo)&&(identical(other.content, _this.content) || other.content == _this.content)&&const DeepCollectionEquality().equals(other.attachments, _this.attachments)&&(identical(other.linkPreview, _this.linkPreview) || other.linkPreview == _this.linkPreview));
}


@override
int get hashCode {
  final _this = this as UiMimiContent;
  return Object.hash(runtimeType,_this.plainBody,_this.previewText,const DeepCollectionEquality().hash(_this.replaces),const DeepCollectionEquality().hash(_this.topicId),const DeepCollectionEquality().hash(_this.inReplyTo),_this.content,const DeepCollectionEquality().hash(_this.attachments),_this.linkPreview);
}

@override
String toString() {
  final _this = this as UiMimiContent;
  return 'UiMimiContent(plainBody: ${_this.plainBody}, previewText: ${_this.previewText}, replaces: ${_this.replaces}, topicId: ${_this.topicId}, inReplyTo: ${_this.inReplyTo}, content: ${_this.content}, attachments: ${_this.attachments}, linkPreview: ${_this.linkPreview})';
}


}

/// @nodoc
abstract mixin class $UiMimiContentCopyWith<$Res>  {
  factory $UiMimiContentCopyWith(UiMimiContent value, $Res Function(UiMimiContent) _then) = _$UiMimiContentCopyWithImpl;
@useResult
$Res call({
 String? plainBody, String? previewText, Uint8List? replaces, Uint8List topicId, Uint8List? inReplyTo, MessageContent? content, List<UiAttachment> attachments, UiLinkPreview? linkPreview
});


$MessageContentCopyWith<$Res>? get content;$UiLinkPreviewCopyWith<$Res>? get linkPreview;

}
/// @nodoc
class _$UiMimiContentCopyWithImpl<$Res>
    implements $UiMimiContentCopyWith<$Res> {
  _$UiMimiContentCopyWithImpl(this._self, this._then);

  final UiMimiContent _self;
  final $Res Function(UiMimiContent) _then;

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? plainBody = freezed,Object? previewText = freezed,Object? replaces = freezed,Object? topicId = null,Object? inReplyTo = freezed,Object? content = freezed,Object? attachments = null,Object? linkPreview = freezed,}) {
  return _then(UiMimiContent(
plainBody: freezed == plainBody ? _self.plainBody : plainBody // ignore: cast_nullable_to_non_nullable
as String?,previewText: freezed == previewText ? _self.previewText : previewText // ignore: cast_nullable_to_non_nullable
as String?,replaces: freezed == replaces ? _self.replaces : replaces // ignore: cast_nullable_to_non_nullable
as Uint8List?,topicId: null == topicId ? _self.topicId : topicId // ignore: cast_nullable_to_non_nullable
as Uint8List,inReplyTo: freezed == inReplyTo ? _self.inReplyTo : inReplyTo // ignore: cast_nullable_to_non_nullable
as Uint8List?,content: freezed == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as MessageContent?,attachments: null == attachments ? _self.attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiAttachment>,linkPreview: freezed == linkPreview ? _self.linkPreview : linkPreview // ignore: cast_nullable_to_non_nullable
as UiLinkPreview?,
  ));
}
/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res>? get content {
    if (_self.content == null) {
    return null;
  }

  return $MessageContentCopyWith<$Res>(_self.content!, (value) {
    return _then(_self.copyWith(content: value));
  });
}/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewCopyWith<$Res>? get linkPreview {
    if (_self.linkPreview == null) {
    return null;
  }

  return $UiLinkPreviewCopyWith<$Res>(_self.linkPreview!, (value) {
    return _then(_self.copyWith(linkPreview: value));
  });
}
}



/// @nodoc


class _UiMimiContent implements UiMimiContent {
  const _UiMimiContent({this.plainBody, this.previewText, this.replaces, required this.topicId, this.inReplyTo, this.content, required  List<UiAttachment> attachments, this.linkPreview}): _attachments = attachments;
  

@override final  String? plainBody;
@override final  String? previewText;
@override final  Uint8List? replaces;
@override final  Uint8List topicId;
@override final  Uint8List? inReplyTo;
@override final  MessageContent? content;
 final  List<UiAttachment> _attachments;
@override List<UiAttachment> get attachments {
  if (_attachments is EqualUnmodifiableListView) return _attachments;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_attachments);
}

@override final  UiLinkPreview? linkPreview;

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiMimiContentCopyWith<_UiMimiContent> get copyWith => __$UiMimiContentCopyWithImpl<_UiMimiContent>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiMimiContent&&(identical(other.plainBody, plainBody) || other.plainBody == plainBody)&&(identical(other.previewText, previewText) || other.previewText == previewText)&&const DeepCollectionEquality().equals(other.replaces, replaces)&&const DeepCollectionEquality().equals(other.topicId, topicId)&&const DeepCollectionEquality().equals(other.inReplyTo, inReplyTo)&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other.attachments, _attachments)&&(identical(other.linkPreview, linkPreview) || other.linkPreview == linkPreview));
}


@override
int get hashCode {
    return Object.hash(runtimeType,plainBody,previewText,const DeepCollectionEquality().hash(replaces),const DeepCollectionEquality().hash(topicId),const DeepCollectionEquality().hash(inReplyTo),content,const DeepCollectionEquality().hash(_attachments),linkPreview);
}

@override
String toString() {
    return 'UiMimiContent(plainBody: $plainBody, previewText: $previewText, replaces: $replaces, topicId: $topicId, inReplyTo: $inReplyTo, content: $content, attachments: $attachments, linkPreview: $linkPreview)';
}


}

/// @nodoc
abstract mixin class _$UiMimiContentCopyWith<$Res> implements $UiMimiContentCopyWith<$Res> {
  factory _$UiMimiContentCopyWith(_UiMimiContent value, $Res Function(_UiMimiContent) _then) = __$UiMimiContentCopyWithImpl;
@override @useResult
$Res call({
 String? plainBody, String? previewText, Uint8List? replaces, Uint8List topicId, Uint8List? inReplyTo, MessageContent? content, List<UiAttachment> attachments, UiLinkPreview? linkPreview
});


@override $MessageContentCopyWith<$Res>? get content;@override $UiLinkPreviewCopyWith<$Res>? get linkPreview;

}
/// @nodoc
class __$UiMimiContentCopyWithImpl<$Res>
    implements _$UiMimiContentCopyWith<$Res> {
  __$UiMimiContentCopyWithImpl(this._self, this._then);

  final _UiMimiContent _self;
  final $Res Function(_UiMimiContent) _then;

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? plainBody = freezed,Object? previewText = freezed,Object? replaces = freezed,Object? topicId = null,Object? inReplyTo = freezed,Object? content = freezed,Object? attachments = null,Object? linkPreview = freezed,}) {
  return _then(_UiMimiContent(
plainBody: freezed == plainBody ? _self.plainBody : plainBody // ignore: cast_nullable_to_non_nullable
as String?,previewText: freezed == previewText ? _self.previewText : previewText // ignore: cast_nullable_to_non_nullable
as String?,replaces: freezed == replaces ? _self.replaces : replaces // ignore: cast_nullable_to_non_nullable
as Uint8List?,topicId: null == topicId ? _self.topicId : topicId // ignore: cast_nullable_to_non_nullable
as Uint8List,inReplyTo: freezed == inReplyTo ? _self.inReplyTo : inReplyTo // ignore: cast_nullable_to_non_nullable
as Uint8List?,content: freezed == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as MessageContent?,attachments: null == attachments ? _self._attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiAttachment>,linkPreview: freezed == linkPreview ? _self.linkPreview : linkPreview // ignore: cast_nullable_to_non_nullable
as UiLinkPreview?,
  ));
}

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res>? get content {
    if (_self.content == null) {
    return null;
  }

  return $MessageContentCopyWith<$Res>(_self.content!, (value) {
    return _then(_self.copyWith(content: value));
  });
}/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiLinkPreviewCopyWith<$Res>? get linkPreview {
    if (_self.linkPreview == null) {
    return null;
  }

  return $UiLinkPreviewCopyWith<$Res>(_self.linkPreview!, (value) {
    return _then(_self.copyWith(linkPreview: value));
  });
}
}

/// @nodoc
mixin _$UiMimiId {

 U8Array32 get field0;
/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiMimiIdCopyWith<UiMimiId> get copyWith => _$UiMimiIdCopyWithImpl<UiMimiId>(this as UiMimiId, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiMimiId;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiMimiId&&const DeepCollectionEquality().equals(other.field0, _this.field0));
}


@override
int get hashCode {
  final _this = this as UiMimiId;
  return Object.hash(runtimeType,const DeepCollectionEquality().hash(_this.field0));
}

@override
String toString() {
  final _this = this as UiMimiId;
  return 'UiMimiId(field0: ${_this.field0})';
}


}

/// @nodoc
abstract mixin class $UiMimiIdCopyWith<$Res>  {
  factory $UiMimiIdCopyWith(UiMimiId value, $Res Function(UiMimiId) _then) = _$UiMimiIdCopyWithImpl;
@useResult
$Res call({
 U8Array32 field0
});




}
/// @nodoc
class _$UiMimiIdCopyWithImpl<$Res>
    implements $UiMimiIdCopyWith<$Res> {
  _$UiMimiIdCopyWithImpl(this._self, this._then);

  final UiMimiId _self;
  final $Res Function(UiMimiId) _then;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? field0 = null,}) {
  return _then(UiMimiId(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as U8Array32,
  ));
}

}



/// @nodoc


class _UiMimiId implements UiMimiId {
  const _UiMimiId({required this.field0});
  

@override final  U8Array32 field0;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiMimiIdCopyWith<_UiMimiId> get copyWith => __$UiMimiIdCopyWithImpl<_UiMimiId>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiMimiId&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));
}

@override
String toString() {
    return 'UiMimiId(field0: $field0)';
}


}

/// @nodoc
abstract mixin class _$UiMimiIdCopyWith<$Res> implements $UiMimiIdCopyWith<$Res> {
  factory _$UiMimiIdCopyWith(_UiMimiId value, $Res Function(_UiMimiId) _then) = __$UiMimiIdCopyWithImpl;
@override @useResult
$Res call({
 U8Array32 field0
});




}
/// @nodoc
class __$UiMimiIdCopyWithImpl<$Res>
    implements _$UiMimiIdCopyWith<$Res> {
  __$UiMimiIdCopyWithImpl(this._self, this._then);

  final _UiMimiId _self;
  final $Res Function(_UiMimiId) _then;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(_UiMimiId(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as U8Array32,
  ));
}


}

// dart format on
