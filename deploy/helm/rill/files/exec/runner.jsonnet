// bb_runner: runs each action in /worker/build, inside gVisor, with no
// network. The worker reaches it over /worker/runner.
{
  buildDirectoryPath: '/worker/build',
  // Between actions /tmp is emptied and leftover processes are killed,
  // so one action cannot leave anything behind for the next.
  cleanTemporaryDirectories: ['/tmp'],
  cleanProcessTable: true,
  grpcServers: [{
    listenPaths: ['/worker/runner'],
    authenticationPolicy: { allow: {} },
  }],
}
