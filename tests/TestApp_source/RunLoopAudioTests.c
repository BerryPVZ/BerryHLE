/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>
#include <sys/time.h>

typedef unsigned int UInt32;
typedef int OSStatus;
typedef const void *CFRunLoopRef;
typedef const void *CFStringRef;
typedef struct OpaqueAudioQueue *AudioQueueRef;
typedef struct {
  UInt32 mAudioDataBytesCapacity;
  void *mAudioData;
  UInt32 mAudioDataByteSize;
  void *mUserData;
} AudioQueueBuffer;
typedef AudioQueueBuffer *AudioQueueBufferRef;
typedef struct {
  double mSampleRate;
  UInt32 mFormatID, mFormatFlags, mBytesPerPacket, mFramesPerPacket,
      mBytesPerFrame, mChannelsPerFrame, mBitsPerChannel, mReserved;
} AudioStreamBasicDescription;
typedef void (*AudioQueueOutputCallback)(void *, AudioQueueRef, AudioQueueBufferRef);
OSStatus AudioQueueNewOutput(const AudioStreamBasicDescription *, AudioQueueOutputCallback,
                             void *, CFRunLoopRef, CFStringRef, UInt32, AudioQueueRef *);
OSStatus AudioQueueAllocateBuffer(AudioQueueRef, UInt32, AudioQueueBufferRef *);
OSStatus AudioQueueEnqueueBuffer(AudioQueueRef, AudioQueueBufferRef, UInt32, const void *);
OSStatus AudioQueueStart(AudioQueueRef, const void *);
CFRunLoopRef CFRunLoopGetCurrent(void);
int CFRunLoopRunInMode(CFStringRef, double, unsigned char);
void CFRunLoopAddCommonMode(CFRunLoopRef, CFStringRef);
OSStatus AudioQueueStop(AudioQueueRef, unsigned char);
OSStatus AudioQueueDispose(AudioQueueRef, unsigned char);
extern CFStringRef CFStringCreateWithCString(void *, const char *, unsigned int);
extern void CFRelease(const void *);
extern CFStringRef kCFRunLoopDefaultMode, kCFRunLoopCommonModes;


typedef struct {
  long version;
  void *info, *retain, *release, *copyDescription, *equal, *hash, *schedule, *cancel;
  void (*perform)(void *);
} CFRunLoopSourceContext;
void *CFRunLoopSourceCreate(void *, long, const CFRunLoopSourceContext *);
void CFRunLoopAddSource(CFRunLoopRef, void *, CFStringRef);
void CFRunLoopRemoveSource(CFRunLoopRef, void *, CFStringRef);
static void idle_audio_source(void *info) {}

static int callbacks, audio_result;
static void audio_callback(void *context, AudioQueueRef queue,
                           AudioQueueBufferRef buffer) {
  callbacks++;
  if (AudioQueueEnqueueBuffer(queue, buffer, 0, NULL)) audio_result = -6;
}
static void *audio_worker(void *independent) {
  AudioStreamBasicDescription format = {44100, 0x6c70636d, 12, 2, 1, 2, 1, 16, 0};
  AudioQueueRef queue;
  if (AudioQueueNewOutput(&format, audio_callback, NULL,
                          independent ? NULL : CFRunLoopGetCurrent(),
                          kCFRunLoopCommonModes, 0, &queue)) {
    audio_result = -1; return NULL;
  }
  for (int i = 0; i < 3; i++) {
    AudioQueueBufferRef buffer;
    if (AudioQueueAllocateBuffer(queue, 1024, &buffer)) {
      audio_result = -2; goto dispose;
    }
    memset(buffer->mAudioData, 0, 1024);
    buffer->mAudioDataByteSize = 1024;
    if (AudioQueueEnqueueBuffer(queue, buffer, 0, NULL)) {
      audio_result = -3; goto dispose;
    }
  }
  // The Rust runner forces the silent OpenAL backend: no physical device.
  if (AudioQueueStart(queue, NULL)) { audio_result = -4; goto dispose; }
  CFStringRef custom = CFStringCreateWithCString(NULL, "TestAppAudioMode", 0x08000100);
  if (CFRunLoopRunInMode(custom, 0, 0) != 1) audio_result = -7;
  if (independent) {
    // NULL queues must neither depend on this custom mode nor keep it alive.
    if (CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0, 0) != 1) audio_result = -16;
    CFRunLoopSourceContext context = {0};
    context.perform = idle_audio_source;
    void *source = CFRunLoopSourceCreate(NULL, 0, &context);
    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, custom);
    int result = CFRunLoopRunInMode(custom, 0.3, 0);
    if (result != 3 || callbacks == 0) {
      printf("NULL AudioQueue: custom-mode result=%d callbacks=%d\n", result, callbacks);
      if (audio_result == 0) audio_result = -17;
    }
    CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, custom);
    CFRelease(source);
    CFRelease(custom);
    if (AudioQueueStop(queue, 1) && audio_result == 0) audio_result = -11;
    goto dispose;
  }
  int result = CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.15, 0);
  if (result == 1) audio_result = -8; // registered queue must keep the mode alive
  else if (result != 3 || callbacks == 0) audio_result = -9;
  int previous = callbacks;
  CFRunLoopAddCommonMode(CFRunLoopGetCurrent(), custom);
  result = CFRunLoopRunInMode(custom, 0.15, 0);
  if (audio_result == 0 && (result != 3 || callbacks <= previous)) audio_result = -10;
  CFRelease(custom);
  if (AudioQueueStop(queue, 1) && audio_result == 0) audio_result = -11;
dispose:
  if (AudioQueueDispose(queue, 1) && audio_result == 0) audio_result = -12;
  if (CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0, 0) != 1 && audio_result == 0)
    audio_result = -13;
  return NULL;
}
int test_runloop_audio_queue(void) {
  pthread_t thread;
  callbacks = audio_result = 0;
  if (pthread_create(&thread, NULL, audio_worker, NULL)) return -14;
  if (pthread_join(thread, NULL)) return -15;
  return audio_result;
}

int test_runloop_audio_queue_null(void) {
  callbacks = audio_result = 0;
  audio_worker((void *)1); // main thread, custom mode, no real audio device
  return audio_result;
}

// Public Audio Unit ABI, absent from the old test SDK.
typedef void *AudioUnit;
typedef struct { UInt32 type, subtype, manufacturer, flags, mask; } ComponentDescription;
typedef struct { UInt32 channels, size; void *data; } AudioBuffer;
typedef struct { UInt32 count; AudioBuffer buffers[1]; } AudioBufferList;
typedef OSStatus (*RenderCallback)(void *, UInt32 *, const void *, UInt32,
                                   UInt32, AudioBufferList *);
typedef struct { RenderCallback callback; void *context; } RenderCallbackStruct;
void *AudioComponentFindNext(void *, const ComponentDescription *);
OSStatus AudioComponentInstanceNew(void *, AudioUnit *);
OSStatus AudioComponentInstanceDispose(AudioUnit);
OSStatus AudioUnitSetProperty(AudioUnit, UInt32, UInt32, UInt32, const void *, UInt32);
OSStatus AudioUnitInitialize(AudioUnit);
OSStatus AudioUnitUninitialize(AudioUnit);
OSStatus AudioOutputUnitStart(AudioUnit);
OSStatus AudioOutputUnitStop(AudioUnit);
static int unit_callbacks;
static OSStatus unit_render(void *context, UInt32 *flags, const void *timestamp,
                            UInt32 bus, UInt32 frames, AudioBufferList *data) {
  unit_callbacks++;
  for (UInt32 i = 0; i < data->count; i++)
    if (data->buffers[i].data)
      memset(data->buffers[i].data, 0, data->buffers[i].size);
  return 0;
}
int test_runloop_audio_unit_independent(void) {
  ComponentDescription description = {0x61756f75, 0x72696f63, 0x6170706c, 0, 0};
  void *component = AudioComponentFindNext(NULL, &description);
  if (!component) return -1;
  AudioUnit unit;
  if (AudioComponentInstanceNew(component, &unit)) return -2;
  int result = 0, initialized = 0, started = 0;
  RenderCallbackStruct callback = {unit_render, NULL};
  AudioStreamBasicDescription format = {44100, 0x6c70636d, 12, 2, 1, 2, 1, 16, 0};
  if (AudioUnitSetProperty(unit, 23, 1, 0, &callback, sizeof(callback)) ||
      AudioUnitSetProperty(unit, 8, 1, 0, &format, sizeof(format))) {
    result = -3; goto dispose;
  }
  if (AudioUnitInitialize(unit)) { result = -4; goto dispose; }
  initialized = 1;
  if (CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0, 0) != 1) {
    result = -5; goto dispose;
  }
  if (AudioOutputUnitStart(unit)) { result = -6; goto dispose; }
  started = 1;
  unit_callbacks = 0;
  CFStringRef custom = CFStringCreateWithCString(NULL, "TestAppAudioUnitMode", 0x08000100);
  CFRunLoopSourceContext context = {0};
  context.perform = idle_audio_source;
  void *source = CFRunLoopSourceCreate(NULL, 0, &context);
  CFRunLoopAddSource(CFRunLoopGetCurrent(), source, custom);
  int ran = CFRunLoopRunInMode(custom, 0.3, 0);
  if (ran != 3 || unit_callbacks == 0) result = -7;
  CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, custom);
  CFRelease(source);
  CFRelease(custom);
dispose:
  if (started && AudioOutputUnitStop(unit) && result == 0) result = -8;
  if (initialized && AudioUnitUninitialize(unit) && result == 0) result = -9;
  if (AudioComponentInstanceDispose(unit) && result == 0) result = -10;
  return result;
}
