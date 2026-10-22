// bb_worker against Rill: Rill is the scheduler, the CAS and the action
// cache. RILL_ENDPOINT and RILL_WORKER_SECRET come from the environment.
local rill = {
  address: std.extVar('RILL_ENDPOINT'),
  addMetadata: [{ header: 'authorization', values: ['Bearer rwk_' + std.extVar('RILL_WORKER_SECRET')] }],
};

{
  blobstore: {
    contentAddressableStorage: { grpc: { client: rill } },
    actionCache: { grpc: { client: rill } },
  },
  maximumMessageSizeBytes: 16 * 1024 * 1024,
  scheduler: rill,
  buildDirectories: [{
    native: {
      buildDirectoryPath: '/worker/build',
      cacheDirectoryPath: '/worker/cache',
      maximumCacheFileCount: 100000,
      maximumCacheSizeBytes: 4 * 1024 * 1024 * 1024,
      cacheReplacementPolicy: 'LEAST_RECENTLY_USED',
    },
    runners: [{
      endpoint: { address: 'unix:///worker/runner' },
      concurrency: std.parseInt(std.extVar('RILL_WORKER_CONCURRENCY')),
      instanceNamePrefix: '',
      platform: {
        properties: [
          { name: 'OSFamily', value: 'linux' },
          { name: 'container-image', value: 'rill-exec-runner' },
        ],
      },
      workerId: { hostname: std.extVar('RILL_WORKER_NAME') },
    }],
  }],
  inputDownloadConcurrency: 16,
  outputUploadConcurrency: 16,
  directoryCache: {
    maximumCount: 10000,
    maximumSizeBytes: 16 * 1024 * 1024,
    cacheReplacementPolicy: 'LEAST_RECENTLY_USED',
  },
}
