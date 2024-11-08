import {cli} from './cli';

cli()
  .parseAsync(process.argv)
  .catch(err => {
    throw err;
  });
