// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'pending_share.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$PendingShare {

/// Files extracted by the share activity. Whoever holds the share owns
/// them: the composer once it stages them, the navigation cubit until
/// then.
 List<UiSharedAttachment> get attachments;/// Text shared when not sharing a file (could also be both).
 String? get text;/// Number of shared items the share activity could not hand over.
 int get droppedAttachments;
/// Create a copy of PendingShare
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PendingShareCopyWith<PendingShare> get copyWith => _$PendingShareCopyWithImpl<PendingShare>(this as PendingShare, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PendingShare&&const DeepCollectionEquality().equals(other.attachments, attachments)&&(identical(other.text, text) || other.text == text)&&(identical(other.droppedAttachments, droppedAttachments) || other.droppedAttachments == droppedAttachments));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(attachments),text,droppedAttachments);

@override
String toString() {
  return 'PendingShare(attachments: $attachments, text: $text, droppedAttachments: $droppedAttachments)';
}


}

/// @nodoc
abstract mixin class $PendingShareCopyWith<$Res>  {
  factory $PendingShareCopyWith(PendingShare value, $Res Function(PendingShare) _then) = _$PendingShareCopyWithImpl;
@useResult
$Res call({
 List<UiSharedAttachment> attachments, String? text, int droppedAttachments
});




}
/// @nodoc
class _$PendingShareCopyWithImpl<$Res>
    implements $PendingShareCopyWith<$Res> {
  _$PendingShareCopyWithImpl(this._self, this._then);

  final PendingShare _self;
  final $Res Function(PendingShare) _then;

/// Create a copy of PendingShare
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachments = null,Object? text = freezed,Object? droppedAttachments = null,}) {
  return _then(_self.copyWith(
attachments: null == attachments ? _self.attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiSharedAttachment>,text: freezed == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String?,droppedAttachments: null == droppedAttachments ? _self.droppedAttachments : droppedAttachments // ignore: cast_nullable_to_non_nullable
as int,
  ));
}

}



/// @nodoc


class _PendingShare extends PendingShare {
  const _PendingShare({final  List<UiSharedAttachment> attachments = const <UiSharedAttachment>[], this.text, this.droppedAttachments = 0}): _attachments = attachments,super._();
  

/// Files extracted by the share activity. Whoever holds the share owns
/// them: the composer once it stages them, the navigation cubit until
/// then.
 final  List<UiSharedAttachment> _attachments;
/// Files extracted by the share activity. Whoever holds the share owns
/// them: the composer once it stages them, the navigation cubit until
/// then.
@override@JsonKey() List<UiSharedAttachment> get attachments {
  if (_attachments is EqualUnmodifiableListView) return _attachments;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_attachments);
}

/// Text shared when not sharing a file (could also be both).
@override final  String? text;
/// Number of shared items the share activity could not hand over.
@override@JsonKey() final  int droppedAttachments;

/// Create a copy of PendingShare
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$PendingShareCopyWith<_PendingShare> get copyWith => __$PendingShareCopyWithImpl<_PendingShare>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _PendingShare&&const DeepCollectionEquality().equals(other._attachments, _attachments)&&(identical(other.text, text) || other.text == text)&&(identical(other.droppedAttachments, droppedAttachments) || other.droppedAttachments == droppedAttachments));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_attachments),text,droppedAttachments);

@override
String toString() {
  return 'PendingShare(attachments: $attachments, text: $text, droppedAttachments: $droppedAttachments)';
}


}

/// @nodoc
abstract mixin class _$PendingShareCopyWith<$Res> implements $PendingShareCopyWith<$Res> {
  factory _$PendingShareCopyWith(_PendingShare value, $Res Function(_PendingShare) _then) = __$PendingShareCopyWithImpl;
@override @useResult
$Res call({
 List<UiSharedAttachment> attachments, String? text, int droppedAttachments
});




}
/// @nodoc
class __$PendingShareCopyWithImpl<$Res>
    implements _$PendingShareCopyWith<$Res> {
  __$PendingShareCopyWithImpl(this._self, this._then);

  final _PendingShare _self;
  final $Res Function(_PendingShare) _then;

/// Create a copy of PendingShare
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachments = null,Object? text = freezed,Object? droppedAttachments = null,}) {
  return _then(_PendingShare(
attachments: null == attachments ? _self._attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiSharedAttachment>,text: freezed == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String?,droppedAttachments: null == droppedAttachments ? _self.droppedAttachments : droppedAttachments // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

// dart format on
