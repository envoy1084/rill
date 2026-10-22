// bb_runner: runs each action in /worker/build, inside gVisor, with no
// network. The worker reaches it over /worker/runner.
{
  buildDirectoryPath: '/worker/build',
  grpcServers: [{
    listenPaths: ['/worker/runner'],
    authenticationPolicy: { allow: {} },
  }],
}
