import React from 'react';
import clsx from 'clsx';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  Svg: React.ComponentType<React.ComponentProps<'svg'>>;
  emoji: string;
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Omnipotent',
    Svg: require('@site/static/img/omnipotent.svg').default,
    description: (
      <>
        Omni was designed from the ground up to wrap easily any set of commands
        and provide a consistent interface to interact with them. Write any command,
        make it available in a git repository, and omni will take care of the rest.
      </>
    ),
  },
  {
    title: 'Omniscient',
    Svg: require('@site/static/img/omniscient.svg').default,
    description: (
      <>
        Omni is built to know what you need, when you need it. From easy lookup
        of your git repositories and easy discoverability of (up to date!) commands,
        to dynamically-loaded environment, omni is there to help you.
      </>
    ),
  },
  {
    title: 'Omnipresent',
    Svg: require('@site/static/img/omnipresent.svg').default,
    description: (
      <>
        Add omni to your path, and enjoy calling any of your commands from
        anywhere in your system. Omni will be there for you, no matter your
        current working directory.
      </>
    ),
  },
];

function Feature({title, Svg, emoji, description}: FeatureItem) {
  // Check if an emoji is provided
  if (Svg === undefined) {
    // Render the emoji in a block with larger size
    return (
      <div className={clsx('col col--4')}>
        <div className="text--center">
          <span className={styles.featureEmoji} role="img" aria-label={title}>
            {emoji}
          </span>
        </div>
        <div className="text--center padding-horiz--md">
          <h3>{title}</h3>
          <p>{description}</p>
        </div>
      </div>
    );
  } else {
    // Render the content with the SVG
    return (
      <div className={clsx('col col--4')}>
        <div className="text--center">
          <Svg className={styles.featureSvg} role="img" />
        </div>
        <div className="text--center padding-horiz--md">
          <h3>{title}</h3>
          <p>{description}</p>
        </div>
      </div>
    );
  }
}

export default function HomepageFeatures(): JSX.Element {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
