// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'linked_devices_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$LinkedDevicesState {

 List<UiLinkedDevice> get devices;
/// Create a copy of LinkedDevicesState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$LinkedDevicesStateCopyWith<LinkedDevicesState> get copyWith => _$LinkedDevicesStateCopyWithImpl<LinkedDevicesState>(this as LinkedDevicesState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is LinkedDevicesState&&const DeepCollectionEquality().equals(other.devices, devices));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(devices));

@override
String toString() {
  return 'LinkedDevicesState(devices: $devices)';
}


}

/// @nodoc
abstract mixin class $LinkedDevicesStateCopyWith<$Res>  {
  factory $LinkedDevicesStateCopyWith(LinkedDevicesState value, $Res Function(LinkedDevicesState) _then) = _$LinkedDevicesStateCopyWithImpl;
@useResult
$Res call({
 List<UiLinkedDevice> devices
});




}
/// @nodoc
class _$LinkedDevicesStateCopyWithImpl<$Res>
    implements $LinkedDevicesStateCopyWith<$Res> {
  _$LinkedDevicesStateCopyWithImpl(this._self, this._then);

  final LinkedDevicesState _self;
  final $Res Function(LinkedDevicesState) _then;

/// Create a copy of LinkedDevicesState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? devices = null,}) {
  return _then(_self.copyWith(
devices: null == devices ? _self.devices : devices // ignore: cast_nullable_to_non_nullable
as List<UiLinkedDevice>,
  ));
}

}



/// @nodoc


class _LinkedDevicesState extends LinkedDevicesState {
  const _LinkedDevicesState({required final  List<UiLinkedDevice> devices}): _devices = devices,super._();
  

 final  List<UiLinkedDevice> _devices;
@override List<UiLinkedDevice> get devices {
  if (_devices is EqualUnmodifiableListView) return _devices;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_devices);
}


/// Create a copy of LinkedDevicesState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$LinkedDevicesStateCopyWith<_LinkedDevicesState> get copyWith => __$LinkedDevicesStateCopyWithImpl<_LinkedDevicesState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _LinkedDevicesState&&const DeepCollectionEquality().equals(other._devices, _devices));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_devices));

@override
String toString() {
  return 'LinkedDevicesState(devices: $devices)';
}


}

/// @nodoc
abstract mixin class _$LinkedDevicesStateCopyWith<$Res> implements $LinkedDevicesStateCopyWith<$Res> {
  factory _$LinkedDevicesStateCopyWith(_LinkedDevicesState value, $Res Function(_LinkedDevicesState) _then) = __$LinkedDevicesStateCopyWithImpl;
@override @useResult
$Res call({
 List<UiLinkedDevice> devices
});




}
/// @nodoc
class __$LinkedDevicesStateCopyWithImpl<$Res>
    implements _$LinkedDevicesStateCopyWith<$Res> {
  __$LinkedDevicesStateCopyWithImpl(this._self, this._then);

  final _LinkedDevicesState _self;
  final $Res Function(_LinkedDevicesState) _then;

/// Create a copy of LinkedDevicesState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? devices = null,}) {
  return _then(_LinkedDevicesState(
devices: null == devices ? _self._devices : devices // ignore: cast_nullable_to_non_nullable
as List<UiLinkedDevice>,
  ));
}


}

/// @nodoc
mixin _$UiLinkedDevice {

 String get clientId; String get name; LinkedDevicePlatform get platform; DateTime? get linkedAt; bool get isThisDevice;
/// Create a copy of UiLinkedDevice
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiLinkedDeviceCopyWith<UiLinkedDevice> get copyWith => _$UiLinkedDeviceCopyWithImpl<UiLinkedDevice>(this as UiLinkedDevice, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiLinkedDevice&&(identical(other.clientId, clientId) || other.clientId == clientId)&&(identical(other.name, name) || other.name == name)&&(identical(other.platform, platform) || other.platform == platform)&&(identical(other.linkedAt, linkedAt) || other.linkedAt == linkedAt)&&(identical(other.isThisDevice, isThisDevice) || other.isThisDevice == isThisDevice));
}


@override
int get hashCode => Object.hash(runtimeType,clientId,name,platform,linkedAt,isThisDevice);

@override
String toString() {
  return 'UiLinkedDevice(clientId: $clientId, name: $name, platform: $platform, linkedAt: $linkedAt, isThisDevice: $isThisDevice)';
}


}

/// @nodoc
abstract mixin class $UiLinkedDeviceCopyWith<$Res>  {
  factory $UiLinkedDeviceCopyWith(UiLinkedDevice value, $Res Function(UiLinkedDevice) _then) = _$UiLinkedDeviceCopyWithImpl;
@useResult
$Res call({
 String clientId, String name, LinkedDevicePlatform platform, DateTime? linkedAt, bool isThisDevice
});




}
/// @nodoc
class _$UiLinkedDeviceCopyWithImpl<$Res>
    implements $UiLinkedDeviceCopyWith<$Res> {
  _$UiLinkedDeviceCopyWithImpl(this._self, this._then);

  final UiLinkedDevice _self;
  final $Res Function(UiLinkedDevice) _then;

/// Create a copy of UiLinkedDevice
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? clientId = null,Object? name = null,Object? platform = null,Object? linkedAt = freezed,Object? isThisDevice = null,}) {
  return _then(_self.copyWith(
clientId: null == clientId ? _self.clientId : clientId // ignore: cast_nullable_to_non_nullable
as String,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,platform: null == platform ? _self.platform : platform // ignore: cast_nullable_to_non_nullable
as LinkedDevicePlatform,linkedAt: freezed == linkedAt ? _self.linkedAt : linkedAt // ignore: cast_nullable_to_non_nullable
as DateTime?,isThisDevice: null == isThisDevice ? _self.isThisDevice : isThisDevice // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}



/// @nodoc


class _UiLinkedDevice implements UiLinkedDevice {
  const _UiLinkedDevice({required this.clientId, required this.name, required this.platform, this.linkedAt, required this.isThisDevice});
  

@override final  String clientId;
@override final  String name;
@override final  LinkedDevicePlatform platform;
@override final  DateTime? linkedAt;
@override final  bool isThisDevice;

/// Create a copy of UiLinkedDevice
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiLinkedDeviceCopyWith<_UiLinkedDevice> get copyWith => __$UiLinkedDeviceCopyWithImpl<_UiLinkedDevice>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiLinkedDevice&&(identical(other.clientId, clientId) || other.clientId == clientId)&&(identical(other.name, name) || other.name == name)&&(identical(other.platform, platform) || other.platform == platform)&&(identical(other.linkedAt, linkedAt) || other.linkedAt == linkedAt)&&(identical(other.isThisDevice, isThisDevice) || other.isThisDevice == isThisDevice));
}


@override
int get hashCode => Object.hash(runtimeType,clientId,name,platform,linkedAt,isThisDevice);

@override
String toString() {
  return 'UiLinkedDevice(clientId: $clientId, name: $name, platform: $platform, linkedAt: $linkedAt, isThisDevice: $isThisDevice)';
}


}

/// @nodoc
abstract mixin class _$UiLinkedDeviceCopyWith<$Res> implements $UiLinkedDeviceCopyWith<$Res> {
  factory _$UiLinkedDeviceCopyWith(_UiLinkedDevice value, $Res Function(_UiLinkedDevice) _then) = __$UiLinkedDeviceCopyWithImpl;
@override @useResult
$Res call({
 String clientId, String name, LinkedDevicePlatform platform, DateTime? linkedAt, bool isThisDevice
});




}
/// @nodoc
class __$UiLinkedDeviceCopyWithImpl<$Res>
    implements _$UiLinkedDeviceCopyWith<$Res> {
  __$UiLinkedDeviceCopyWithImpl(this._self, this._then);

  final _UiLinkedDevice _self;
  final $Res Function(_UiLinkedDevice) _then;

/// Create a copy of UiLinkedDevice
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? clientId = null,Object? name = null,Object? platform = null,Object? linkedAt = freezed,Object? isThisDevice = null,}) {
  return _then(_UiLinkedDevice(
clientId: null == clientId ? _self.clientId : clientId // ignore: cast_nullable_to_non_nullable
as String,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,platform: null == platform ? _self.platform : platform // ignore: cast_nullable_to_non_nullable
as LinkedDevicePlatform,linkedAt: freezed == linkedAt ? _self.linkedAt : linkedAt // ignore: cast_nullable_to_non_nullable
as DateTime?,isThisDevice: null == isThisDevice ? _self.isThisDevice : isThisDevice // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

// dart format on
