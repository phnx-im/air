// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'attachments_repository.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UiAttachmentStatus {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachmentStatus);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiAttachmentStatus()';
}


}

/// @nodoc
class $UiAttachmentStatusCopyWith<$Res>  {
$UiAttachmentStatusCopyWith(UiAttachmentStatus _, $Res Function(UiAttachmentStatus) __);
}



/// @nodoc


class UiAttachmentStatus_Pending extends UiAttachmentStatus {
  const UiAttachmentStatus_Pending(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachmentStatus_Pending);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiAttachmentStatus.pending()';
}


}




/// @nodoc


class UiAttachmentStatus_Progress extends UiAttachmentStatus {
  const UiAttachmentStatus_Progress(this.field0): super._();
  

 final  BigInt field0;

/// Create a copy of UiAttachmentStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiAttachmentStatus_ProgressCopyWith<UiAttachmentStatus_Progress> get copyWith => _$UiAttachmentStatus_ProgressCopyWithImpl<UiAttachmentStatus_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachmentStatus_Progress&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'UiAttachmentStatus.progress(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiAttachmentStatus_ProgressCopyWith<$Res> implements $UiAttachmentStatusCopyWith<$Res> {
  factory $UiAttachmentStatus_ProgressCopyWith(UiAttachmentStatus_Progress value, $Res Function(UiAttachmentStatus_Progress) _then) = _$UiAttachmentStatus_ProgressCopyWithImpl;
@useResult
$Res call({
 BigInt field0
});




}
/// @nodoc
class _$UiAttachmentStatus_ProgressCopyWithImpl<$Res>
    implements $UiAttachmentStatus_ProgressCopyWith<$Res> {
  _$UiAttachmentStatus_ProgressCopyWithImpl(this._self, this._then);

  final UiAttachmentStatus_Progress _self;
  final $Res Function(UiAttachmentStatus_Progress) _then;

/// Create a copy of UiAttachmentStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(UiAttachmentStatus_Progress(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class UiAttachmentStatus_Completed extends UiAttachmentStatus {
  const UiAttachmentStatus_Completed(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachmentStatus_Completed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiAttachmentStatus.completed()';
}


}




/// @nodoc


class UiAttachmentStatus_Failed extends UiAttachmentStatus {
  const UiAttachmentStatus_Failed(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachmentStatus_Failed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiAttachmentStatus.failed()';
}


}




// dart format on
